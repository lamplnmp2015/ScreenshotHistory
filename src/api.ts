import { invoke } from "@tauri-apps/api/core";

/** One screenshot record as returned by the Rust backend. */
export interface Screenshot {
  id: number;
  file_path: string;
  /** Unix milliseconds. */
  timestamp: number;
  source_app: string | null;
  width: number;
  height: number;
  file_size: number;
  /** OCR full text (may be empty until OCR finishes). */
  ocr_text: string;
  /** 0 = pending, 1 = done, 2 = failed/skipped. */
  ocr_status: number;
  /** data URL of the 200px thumbnail, or null if missing. */
  thumbnail: string | null;
}

export interface Stats {
  total: number;
  ocr_pending: number;
  total_bytes: number;
}

/** Payload of the `ocr-updated` event. */
export interface OcrUpdate {
  id: number;
  ocr_text: string;
  ocr_status: number;
}

export function getHistoryPage(offset: number, limit: number): Promise<Screenshot[]> {
  return invoke("get_history_page", { offset, limit });
}

export function searchByText(keyword: string, limit: number): Promise<Screenshot[]> {
  return invoke("search_by_text", { keyword, limit });
}

/** Full-resolution image as a data URL. */
export function getImage(id: number): Promise<string | null> {
  return invoke("get_image", { id });
}

export function deleteScreenshot(id: number, deleteFile: boolean): Promise<void> {
  return invoke("delete_screenshot", { id, deleteFile });
}

export function openImageFolder(id: number | null): Promise<void> {
  return invoke("open_image_folder", { id });
}

export function getStats(): Promise<Stats> {
  return invoke("get_stats");
}

/** Whether the `tesseract` CLI was found at startup. */
export function ocrAvailable(): Promise<boolean> {
  return invoke("ocr_available");
}

/** Re-run OCR on an existing screenshot synchronously. Returns the new text. */
export function reOcrScreenshot(id: number): Promise<string> {
  return invoke("reocr_screenshot", { id });
}
