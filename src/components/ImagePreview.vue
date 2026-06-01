<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from "vue";
import { getImage, openImageFolder, type Screenshot } from "../api";

const props = defineProps<{ shot: Screenshot }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "delete", id: number, deleteFile: boolean): void;
}>();

const fullSrc = ref<string | null>(null);
const loading = ref(true);
// When true, deleting also removes the image file from disk; default keeps it.
const alsoDeleteFile = ref(false);

// --- Zoom / pan state for the detail image ---
const MIN_ZOOM = 1;
const MAX_ZOOM = 8;
const zoom = ref(1);
const panX = ref(0);
const panY = ref(0);
const stage = ref<HTMLElement | null>(null);

let dragging = false;
let dragStartX = 0;
let dragStartY = 0;
let panStartX = 0;
let panStartY = 0;

function resetView() {
  zoom.value = 1;
  panX.value = 0;
  panY.value = 0;
}

/** Wheel = zoom toward the cursor (keeps the point under the pointer fixed). */
function onWheel(e: WheelEvent) {
  if (!stage.value) return;
  const rect = stage.value.getBoundingClientRect();
  // Cursor position relative to the stage centre (img is centred there).
  const cx = e.clientX - (rect.left + rect.width / 2);
  const cy = e.clientY - (rect.top + rect.height / 2);

  const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
  const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom.value * factor));
  if (next === zoom.value) return;

  // Keep the cursor anchored: tx' = c - (s'/s)*(c - tx)
  const ratio = next / zoom.value;
  panX.value = cx - ratio * (cx - panX.value);
  panY.value = cy - ratio * (cy - panY.value);
  zoom.value = next;

  if (next === MIN_ZOOM) {
    panX.value = 0;
    panY.value = 0;
  }
}

function onMouseDown(e: MouseEvent) {
  if (zoom.value <= MIN_ZOOM) return;
  dragging = true;
  dragStartX = e.clientX;
  dragStartY = e.clientY;
  panStartX = panX.value;
  panStartY = panY.value;
  e.preventDefault();
}

function onMouseMove(e: MouseEvent) {
  if (!dragging) return;
  panX.value = panStartX + (e.clientX - dragStartX);
  panY.value = panStartY + (e.clientY - dragStartY);
}

function onMouseUp() {
  dragging = false;
}

window.addEventListener("mousemove", onMouseMove);
window.addEventListener("mouseup", onMouseUp);

watch(
  () => props.shot.id,
  async (id) => {
    loading.value = true;
    fullSrc.value = null;
    alsoDeleteFile.value = false;
    resetView();
    try {
      fullSrc.value = await getImage(id);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true }
);

function fmtFull(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes()
  )}:${p(d.getSeconds())}`;
}

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

/** Last path segment of file_path, e.g. "1717000000000-uuid.png". */
function fileName(p: string): string {
  if (!p) return "—";
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
window.addEventListener("keydown", onKey);
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", onMouseUp);
});
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="modal">
      <div
        ref="stage"
        class="stage"
        :class="{ zoomed: zoom > 1 }"
        @wheel.prevent="onWheel"
        @mousedown="onMouseDown"
        @dblclick="resetView"
      >
        <div v-if="loading" class="stage-msg">加载原图…</div>
        <img
          v-else-if="fullSrc"
          :src="fullSrc"
          :alt="shot.source_app || ''"
          :style="{
            transform: `translate(${panX}px, ${panY}px) scale(${zoom})`,
          }"
          draggable="false"
        />
        <div v-else class="stage-msg">原图文件丢失</div>
        <div v-if="!loading && fullSrc" class="zoom-badge">
          {{ Math.round(zoom * 100) }}% · 滚轮缩放，双击复位
        </div>
      </div>


      <aside class="side">
        <header class="side-head">
          <h2>详情</h2>
          <button class="x" title="关闭 (Esc)" @click="emit('close')">✕</button>
        </header>

        <dl class="fields">
          <dt>来源</dt>
          <dd>{{ shot.source_app || "未知" }}</dd>
          <dt>文件名</dt>
          <dd class="fname" :title="shot.file_path">{{ fileName(shot.file_path) }}</dd>
          <dt>时间</dt>
          <dd>{{ fmtFull(shot.timestamp) }}</dd>
          <dt>尺寸</dt>
          <dd>{{ shot.width }} × {{ shot.height }}</dd>
          <dt>大小</dt>
          <dd>{{ fmtSize(shot.file_size) }}</dd>
        </dl>

        <div class="ocr">
          <h3>OCR 全文</h3>
          <div class="ocr-body">
            <span v-if="shot.ocr_status === 0" class="muted">识别中…</span>
            <span v-else-if="shot.ocr_status === 2 && !shot.ocr_text" class="muted">未识别 / 无文字</span>
            <pre v-else-if="shot.ocr_text">{{ shot.ocr_text }}</pre>
            <span v-else class="muted">无文字内容</span>
          </div>
        </div>

        <div class="actions">
          <label class="del-opt" :title="alsoDeleteFile ? '将连同磁盘上的原图一起删除' : '仅从历史记录中移除，磁盘上的原图保留'">
            <input type="checkbox" v-model="alsoDeleteFile" />
            同时删除图片文件
          </label>
          <div class="action-row">
            <button @click="openImageFolder(shot.id)">📁 在文件夹中显示</button>
            <button class="danger" @click="emit('delete', shot.id, alsoDeleteFile)">
              {{ alsoDeleteFile ? "🗑 删除记录和图片" : "🗑 删除记录" }}
            </button>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.75);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 24px;
}
.modal {
  display: flex;
  width: 100%;
  max-width: 1100px;
  height: 100%;
  max-height: 760px;
  background: var(--bg-elev);
  border: 1px solid var(--border);
  border-radius: 12px;
  overflow: hidden;
}
.stage {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #0e0f11;
  padding: 16px;
  position: relative;
  overflow: hidden;
}
.stage.zoomed {
  cursor: grab;
}
.stage.zoomed:active {
  cursor: grabbing;
}
.stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  transform-origin: center center;
  will-change: transform;
  user-select: none;
  -webkit-user-drag: none;
}
.zoom-badge {
  position: absolute;
  left: 12px;
  bottom: 12px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-dim);
  background: rgba(0, 0, 0, 0.55);
  border-radius: 6px;
  pointer-events: none;
}
.stage-msg {
  color: var(--text-dim);
}
.side {
  width: 320px;
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 16px 18px;
}
.side-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.side-head h2 {
  margin: 0;
  font-size: 15px;
}
.x {
  background: transparent;
  color: var(--text-dim);
}
.fields {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 14px;
  margin: 16px 0;
  font-size: 13px;
}
.fields dt {
  color: var(--text-dim);
}
.fields dd {
  margin: 0;
  word-break: break-all;
}
.fields dd.fname {
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 12px;
  color: var(--text);
}
.ocr {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.ocr h3 {
  margin: 0 0 8px;
  font-size: 13px;
  color: var(--text-dim);
}
.ocr-body {
  flex: 1;
  overflow-y: auto;
  background: var(--bg-elev-2);
  border-radius: 6px;
  padding: 10px;
  font-size: 12.5px;
}
.ocr-body pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  line-height: 1.5;
}
.muted {
  color: var(--text-dim);
}
.actions {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 16px;
}
.del-opt {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--text-dim);
  cursor: pointer;
  user-select: none;
}
.del-opt input {
  cursor: pointer;
}
.action-row {
  display: flex;
  gap: 10px;
}
.action-row button {
  flex: 1;
}
.danger:hover {
  background: var(--danger);
  color: #fff;
}
</style>
