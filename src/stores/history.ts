import { defineStore } from "pinia";
import {
  getHistoryPage,
  searchByText,
  deleteScreenshot as apiDelete,
  type Screenshot,
  type OcrUpdate,
} from "../api";

const PAGE_SIZE = 40;

interface State {
  items: Screenshot[];
  loading: boolean;
  reachedEnd: boolean;
  offset: number;
  keyword: string;
  searching: boolean;
}

export const useHistoryStore = defineStore("history", {
  state: (): State => ({
    items: [],
    loading: false,
    reachedEnd: false,
    offset: 0,
    keyword: "",
    searching: false,
  }),

  getters: {
    /** Group items by local YYYY-MM-DD for the timeline view. */
    groups(state): { date: string; items: Screenshot[] }[] {
      const map = new Map<string, Screenshot[]>();
      for (const it of state.items) {
        const d = new Date(it.timestamp);
        const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
          d.getDate()
        ).padStart(2, "0")}`;
        if (!map.has(key)) map.set(key, []);
        map.get(key)!.push(it);
      }
      return [...map.entries()].map(([date, items]) => ({ date, items }));
    },
  },

  actions: {
    /** Reset and load the first page of the unfiltered history. */
    async loadFirstPage() {
      this.keyword = "";
      this.searching = false;
      this.items = [];
      this.offset = 0;
      this.reachedEnd = false;
      await this.loadMore();
    },

    /** Load the next page (no-op while searching or already loading). */
    async loadMore() {
      if (this.loading || this.reachedEnd || this.searching) return;
      this.loading = true;
      try {
        const page = await getHistoryPage(this.offset, PAGE_SIZE);
        if (page.length < PAGE_SIZE) this.reachedEnd = true;
        // De-dupe against items already prepended via live events.
        const known = new Set(this.items.map((i) => i.id));
        for (const row of page) if (!known.has(row.id)) this.items.push(row);
        this.offset += page.length;
      } finally {
        this.loading = false;
      }
    },

    /** Run a full-text search; empty keyword falls back to the timeline. */
    async search(keyword: string) {
      const kw = keyword.trim();
      this.keyword = kw;
      if (!kw) {
        await this.loadFirstPage();
        return;
      }
      this.searching = true;
      this.loading = true;
      try {
        this.items = await searchByText(kw, 200);
        this.reachedEnd = true;
      } finally {
        this.loading = false;
      }
    },

    /** Prepend a freshly captured screenshot pushed from the backend. */
    prepend(shot: Screenshot) {
      if (this.searching) return; // don't disturb an active search
      if (this.items.some((i) => i.id === shot.id)) return;
      this.items.unshift(shot);
      this.offset += 1;
    },

    /** Patch OCR text/status when the backend finishes recognition. */
    applyOcr(update: OcrUpdate) {
      const it = this.items.find((i) => i.id === update.id);
      if (it) {
        it.ocr_text = update.ocr_text;
        it.ocr_status = update.ocr_status;
      }
    },

    async remove(id: number, deleteFile = false) {
      await apiDelete(id, deleteFile);
      this.items = this.items.filter((i) => i.id !== id);
    },
  },
});
