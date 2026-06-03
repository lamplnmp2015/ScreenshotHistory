<script setup lang="ts">
import { ref, watch } from "vue";

const props = defineProps<{ ocrReady: boolean }>();
const emit = defineEmits<{ (e: "search", keyword: string): void }>();

const keyword = ref("");
let timer: ReturnType<typeof setTimeout> | null = null;

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
      <svg class="icon" width="14" height="14" viewBox="0 0 20 20" fill="none">
        <circle cx="8.5" cy="8.5" r="5.5" stroke="currentColor" stroke-width="1.8"/>
        <path d="M13 13L17 17" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>
      </svg>
      <input
        v-model="keyword"
        type="text"
        :placeholder="props.ocrReady ? '搜索截图里的文字…' : '搜索来源应用…（OCR 未启用）'"
        spellcheck="false"
      />
      <button v-if="keyword" class="clear" title="清除" @click="clear">
        <svg width="13" height="13" viewBox="0 0 20 20" fill="currentColor">
          <circle cx="10" cy="10" r="9" fill="rgba(0,0,0,0.18)"/>
          <path d="M7 7l6 6M13 7l-6 6" stroke="#fff" stroke-width="1.8" stroke-linecap="round"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.searchbar {
  padding: 10px 14px 8px;
  -webkit-app-region: drag;
}
.field {
  position: relative;
  -webkit-app-region: no-drag;
}
.icon {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-dim);
  pointer-events: none;
}
.field input {
  width: 100%;
  padding-left: 32px;
  padding-right: 30px;
}
.clear {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  padding: 0;
  background: transparent;
  line-height: 0;
  display: flex;
  align-items: center;
}
.clear:hover { background: transparent; }
</style>
