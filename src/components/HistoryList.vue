<script setup lang="ts">
import { computed } from "vue";
import type { Screenshot } from "../api";
import { useHistoryStore } from "../stores/history";

const props = defineProps<{ keyword: string }>();
const emit = defineEmits<{ (e: "open", shot: Screenshot): void }>();

const store = useHistoryStore();
const groups = computed(() => store.groups);

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function fmtDateHeading(date: string): string {
  const today = new Date();
  const y = today.getFullYear();
  const m = String(today.getMonth() + 1).padStart(2, "0");
  const d = String(today.getDate()).padStart(2, "0");
  if (date === `${y}-${m}-${d}`) return "今天";
  const yd = new Date(today.getTime() - 86400000);
  const yKey = `${yd.getFullYear()}-${String(yd.getMonth() + 1).padStart(2, "0")}-${String(
    yd.getDate()
  ).padStart(2, "0")}`;
  if (date === yKey) return "昨天";
  return date;
}

/** Build a short OCR snippet around the matched keyword, with <mark>. */
function snippet(shot: Screenshot): string {
  const text = (shot.ocr_text || "").replace(/\s+/g, " ").trim();
  if (!text) return "";
  const kw = props.keyword.trim();
  if (!kw) return text.slice(0, 80);
  const idx = text.toLowerCase().indexOf(kw.toLowerCase());
  if (idx < 0) return text.slice(0, 80);
  const start = Math.max(0, idx - 24);
  const end = Math.min(text.length, idx + kw.length + 40);
  const before = escapeHtml(text.slice(start, idx));
  const mid = escapeHtml(text.slice(idx, idx + kw.length));
  const after = escapeHtml(text.slice(idx + kw.length, end));
  return `${start > 0 ? "…" : ""}${before}<mark>${mid}</mark>${after}${end < text.length ? "…" : ""}`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 400) {
    store.loadMore();
  }
}
</script>

<template>
  <div class="scroll" @scroll="onScroll">
    <div v-if="store.items.length === 0 && !store.loading" class="empty">
      <p v-if="keyword">没有匹配 “{{ keyword }}” 的截图。</p>
      <p v-else>还没有截图。复制任意图片或按 PrintScreen，新截图会自动出现在这里。</p>
    </div>

    <section v-for="g in groups" :key="g.date" class="group">
      <h3 class="date">{{ fmtDateHeading(g.date) }}</h3>
      <div class="grid">
        <article
          v-for="shot in g.items"
          :key="shot.id"
          class="card"
          @click="emit('open', shot)"
        >
          <div class="thumb">
            <img v-if="shot.thumbnail" :src="shot.thumbnail" :alt="shot.source_app || ''" loading="lazy" />
            <div v-else class="thumb-fallback">无缩略图</div>
            <span class="time">{{ fmtTime(shot.timestamp) }}</span>
          </div>
          <div class="info">
            <span class="app" :title="shot.source_app || ''">{{ shot.source_app || "未知来源" }}</span>
            <span v-if="shot.ocr_status === 0" class="ocr-pending">识别中…</span>
            <p v-else-if="snippet(shot)" class="snip" v-html="snippet(shot)"></p>
          </div>
        </article>
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
  padding: 8px 18px 40px;
}
.group {
  margin-top: 18px;
}
.date {
  position: sticky;
  top: 0;
  z-index: 1;
  margin: 0 0 12px;
  padding: 6px 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-dim);
  background: linear-gradient(var(--bg) 70%, transparent);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  gap: 14px;
}
.card {
  background: var(--bg-elev);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
  cursor: pointer;
  transition: border-color 0.12s ease, transform 0.12s ease;
}
.card:hover {
  border-color: var(--accent);
  transform: translateY(-2px);
}
.thumb {
  position: relative;
  aspect-ratio: 16 / 10;
  background: var(--bg-elev-2);
  display: flex;
  align-items: center;
  justify-content: center;
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.thumb-fallback {
  color: var(--text-dim);
  font-size: 12px;
}
.time {
  position: absolute;
  right: 6px;
  bottom: 6px;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.6);
  color: #fff;
}
.info {
  padding: 8px 10px 10px;
}
.app {
  display: block;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ocr-pending {
  font-size: 11px;
  color: var(--accent);
}
.snip {
  margin: 4px 0 0;
  font-size: 11px;
  line-height: 1.4;
  color: var(--text-dim);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.empty {
  text-align: center;
  color: var(--text-dim);
  margin-top: 80px;
  font-size: 14px;
}
.loading,
.end {
  text-align: center;
  color: var(--text-dim);
  font-size: 12px;
  padding: 20px 0;
}
</style>
