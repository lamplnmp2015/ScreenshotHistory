<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import SearchBar from "./components/SearchBar.vue";
import HistoryList from "./components/HistoryList.vue";
import ImagePreview from "./components/ImagePreview.vue";
import { useHistoryStore } from "./stores/history";
import { ocrAvailable, type Screenshot, type OcrUpdate } from "./api";

const store = useHistoryStore();
const selected = ref<Screenshot | null>(null);
const ocrReady = ref(false);
let unlistenNew: UnlistenFn | null = null;
let unlistenOcr: UnlistenFn | null = null;

onMounted(async () => {
  await store.loadFirstPage();
  ocrReady.value = await ocrAvailable();

  // Live: new screenshot captured by the clipboard monitor (spec §4.5).
  unlistenNew = await listen<Screenshot>("new-screenshot", (e) => {
    store.prepend(e.payload);
  });

  // Live: OCR finished for a previously-captured screenshot.
  unlistenOcr = await listen<OcrUpdate>("ocr-updated", (e) => {
    store.applyOcr(e.payload);
    if (selected.value && selected.value.id === e.payload.id) {
      selected.value.ocr_text = e.payload.ocr_text;
      selected.value.ocr_status = e.payload.ocr_status;
    }
  });
});

onBeforeUnmount(() => {
  unlistenNew?.();
  unlistenOcr?.();
});

function onSearch(keyword: string) {
  store.search(keyword);
}

function openShot(shot: Screenshot) {
  selected.value = shot;
}

async function onDelete(id: number, deleteFile: boolean) {
  await store.remove(id, deleteFile);
  if (selected.value?.id === id) selected.value = null;
}
</script>

<template>
  <div class="app">
    <SearchBar :ocr-ready="ocrReady" @search="onSearch" />
    <main class="body">
      <HistoryList :keyword="store.keyword" @open="openShot" />
    </main>
    <ImagePreview
      v-if="selected"
      :shot="selected"
      @close="selected = null"
      @delete="onDelete"
    />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.body {
  flex: 1;
  min-height: 0;
}
</style>
