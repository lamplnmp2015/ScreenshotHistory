<script setup lang="ts">
import { computed } from "vue";
import type { Screenshot } from "../api";
import { useHistoryStore } from "../stores/history";

const props = defineProps<{ keyword: string }>();
const emit = defineEmits<{ (e: "open", shot: Screenshot): void }>();

const store = useHistoryStore();
const groups = computed(() => store.groups);

/** Strip common executable suffixes: .exe .app .dmg etc. */
function fmtApp(source: string | null): string {
  if (!source) return "未知来源";
  return source.replace(/\.(exe|app|dmg|msi|apk|deb|rpm)$/i, "");
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function fmtDateHeading(date: string): string {
  const today = new Date();
  const key = (dt: Date) =>
    `${dt.getFullYear()}-${String(dt.getMonth() + 1).padStart(2, "0")}-${String(dt.getDate()).padStart(2, "0")}`;
  if (date === key(today)) return "今天";
  if (date === key(new Date(today.getTime() - 86400000))) return "昨天";
  return date;
}

function snippet(shot: Screenshot): string {
  const text = (shot.ocr_text || "").replace(/\s+/g, " ").trim();
  if (!text) return "";
  const kw = props.keyword.trim();
  if (!kw) return text.slice(0, 90);
  const idx = text.toLowerCase().indexOf(kw.toLowerCase());
  if (idx < 0) return text.slice(0, 90);
  const start = Math.max(0, idx - 20);
  const end = Math.min(text.length, idx + kw.length + 50);
  const e = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return `${start > 0 ? "…" : ""}${e(text.slice(start, idx))}<mark>${e(text.slice(idx, idx + kw.length))}</mark>${e(text.slice(idx + kw.length, end))}${end < text.length ? "…" : ""}`;
}

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 400) store.loadMore();
}
</script>

<template>
  <div class="scroll" @scroll="onScroll">
    <div v-if="store.items.length === 0 && !store.loading" class="empty">
      <div class="empty-icon">📷</div>
      <p v-if="keyword">没有匹配 "{{ keyword }}" 的截图</p>
      <p v-else>截图后会自动出现在这里</p>
    </div>

    <section v-for="g in groups" :key="g.date" class="group">
      <h3 class="date-label">{{ fmtDateHeading(g.date) }}</h3>

      <div class="list">
        <div
          v-for="shot in g.items"
          :key="shot.id"
          class="row"
          @click="emit('open', shot)"
        >
          <div class="thumb">
            <img v-if="shot.thumbnail" :src="shot.thumbnail" :alt="fmtApp(shot.source_app)" loading="lazy" />
            <div v-else class="thumb-empty">🖼</div>
          </div>

          <div class="info">
            <div class="meta">
              <span class="time">{{ fmtTime(shot.timestamp) }}</span>
              <span class="sep">·</span>
              <span class="source">{{ fmtApp(shot.source_app) }}</span>
            </div>
            <div v-if="shot.ocr_status === 0" class="ocr pending">识别中…</div>
            <div v-else-if="snippet(shot)" class="ocr" v-html="snippet(shot)"></div>
            <div v-else-if="!snippet(shot)" class="ocr no-text ">暂无被识别的文本</div>
          </div>

          <svg class="chevron" width="7" height="12" viewBox="0 0 7 12" fill="none">
            <path d="M1 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </div>
      </div>
    </section>

    <div v-if="store.loading" class="loading">加载中…</div>
    <div v-else-if="store.reachedEnd && store.items.length > 0 && !keyword" class="end">— 到底了 —</div>
  </div>
</template>

<style scoped>
.scroll {
  height: 100%;
  overflow-y: auto;
  padding: 0 0 40px;
}
.group { margin-top: 4px; }
.date-label {
  position: sticky;
  top: 0;
  z-index: 1;
  margin: 0;
  padding: 8px 16px 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-dim);
  background: var(--bg);
  letter-spacing: 0.02em;
}
.list {
  margin: 0 10px 8px;
  background: var(--bg-elev);
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0,0,0,.08), 0 0 0 0.5px rgba(0,0,0,.06);
  overflow: hidden;
}
.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  cursor: pointer;
  transition: background 0.1s;
  border-bottom: 0.5px solid var(--border-light);
}
.row:last-child { border-bottom: none; }
.row:hover { background: rgba(0,0,0,.03); }
.row:active { background: rgba(0,0,0,.06); }
.thumb {
  width: 100px;
  /* height: 48px; */
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;
  background: var(--bg-elev-2);
  display: flex;
  align-items: center;
  justify-content: center;
}
.thumb img { width: 100%; height: 100%; object-fit: cover; }
.thumb-empty { font-size: 20px; opacity: 0.35; }
.info { 
  flex: 1; min-width: 0; display: flex; justify-content: flex-start;flex-direction: column;
}
.meta {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 3px;
}
.time { font-variant-numeric: tabular-nums; }
.sep { opacity: 0.5; }
.source {
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ocr {
  display: inline-block;
  width: 100%;
  font-size: 13px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.no-text {
  
}
.ocr.pending { color: var(--text-dim); font-style: italic; font-size: 12px; }
.chevron { color: var(--border); flex-shrink: 0; }
.empty { text-align: center; color: var(--text-dim); margin-top: 80px; font-size: 14px; }
.empty-icon { font-size: 36px; margin-bottom: 10px; opacity: 0.4; }
.loading, .end { text-align: center; color: var(--text-dim); font-size: 12px; padding: 20px 0; }
</style>
