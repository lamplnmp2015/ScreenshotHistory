//! Async OCR worker (spec §4.3 + §7). Pulls jobs off a channel, runs the
//! tesseract CLI, writes the result back to SQLite/FTS and notifies the UI.

use crate::{db, ocr_engine, AppPaths};
use std::sync::mpsc::Receiver;
use tauri::{AppHandle, Emitter};

use crate::clipboard_monitor::OcrJob;

#[derive(serde::Serialize, Clone)]
struct OcrUpdate {
    id: i64,
    ocr_text: String,
    ocr_status: i64,
}

/// Spawn the worker thread. Owns its own DB connection.
pub fn start(app: AppHandle, paths: AppPaths, rx: Receiver<OcrJob>, ocr_available: bool) {
    std::thread::spawn(move || {
        let conn = match db::open(&paths.db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[ocr] cannot open db: {e}");
                return;
            }
        };

        for job in rx {
            let (text, status) = if ocr_available {
                match ocr_engine::extract_text(&job.image_path) {
                    Ok(t) => (t, db::OCR_DONE),
                    Err(e) => {
                        eprintln!("[ocr] {}: {e}", job.image_path);
                        (String::new(), db::OCR_FAILED)
                    }
                }
            } else {
                // No engine: mark as failed/skipped so the UI stops spinning.
                (String::new(), db::OCR_FAILED)
            };

            if let Err(e) = db::set_ocr(&conn, job.id, &text, status) {
                eprintln!("[ocr] db update {}: {e}", job.id);
                continue;
            }

            let _ = app.emit(
                "ocr-updated",
                OcrUpdate {
                    id: job.id,
                    ocr_text: text,
                    ocr_status: status,
                },
            );
        }
    });
}
