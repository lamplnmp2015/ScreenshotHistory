<script setup lang="ts">
import { ref, watch } from "vue";

const props = defineProps<{ ocrReady: boolean; total: number }>();
const emit = defineEmits<{ (e: "search", keyword: string): void }>();

const keyword = ref("");
let timer: ReturnType<typeof setTimeout> | null = null;

// Debounced search (300ms per spec §4.5).
watch(keyword, (val) => {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => emit("search", val), 300);
});

function clear() {
  keyword.value = "";
}
</script>

<template>
  <div class="searchbar">
    <div class="field">
      <span class="icon">🔍</span>
      <input
        v-model="keyword"
        type="text"
        :placeholder="props.ocrReady ? '搜索截图中的文字…' : '搜索来源 / 文件名…（OCR 未启用）'"
        spellcheck="false"
      />
      <button v-if="keyword" class="clear" title="清除" @click="clear">✕</button>
    </div>
    <div class="meta">
      <span>{{ props.total }} 张截图</span>
      <span class="dot" :class="{ on: props.ocrReady }" :title="props.ocrReady ? 'OCR 已启用' : 'OCR 未启用 (未检测到 tesseract)'"></span>
    </div>
  </div>
</template>

<style scoped>
.searchbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 18px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-elev);
  -webkit-app-region: drag;
}
.field {
  position: relative;
  flex: 1;
  -webkit-app-region: no-drag;
}
.field .icon {
  position: absolute;
  left: 12px;
  top: 50%;
  transform: translateY(-50%);
  opacity: 0.6;
  font-size: 13px;
}
.field input {
  width: 100%;
  padding-left: 34px;
}
.clear {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  padding: 2px 8px;
  background: transparent;
  color: var(--text-dim);
}
.meta {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-dim);
  font-size: 13px;
  white-space: nowrap;
  -webkit-app-region: no-drag;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-dim);
}
.dot.on {
  background: #51cf66;
}
</style>
