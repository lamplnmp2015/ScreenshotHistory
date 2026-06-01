//! Background clipboard monitor (spec §4.1): poll every 500ms, dedupe by hash,
//! and on a new image -> save + insert + emit `new-screenshot` + enqueue OCR.

use crate::{db, foreground, image_saver, AppPaths};
use arboard::Clipboard;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// A unit of OCR work handed to the worker thread.
pub struct OcrJob {
    pub id: i64,
    pub image_path: String,
}

/// Spawn the polling thread. Owns its own DB connection.
pub fn start(app: AppHandle, paths: AppPaths, ocr_tx: Sender<OcrJob>) {
    std::thread::spawn(move || {
        let conn = match db::open(&paths.db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[clipboard] cannot open db: {e}");
                return;
            }
        };

        let mut clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[clipboard] cannot access clipboard: {e}");
                return;
            }
        };

        let mut last_hash = String::new();
        loop {
            if let Ok(img) = clipboard.get_image() {
                // Hash the raw pixels so identical captures are skipped cheaply.
                let hash = format!("{:x}", md5::compute(&img.bytes));
                if hash != last_hash {
                    last_hash = hash.clone();
                    // Only keep genuine screenshots. A *copied* image (from a
                    // file, browser, viewer) carries companion clipboard formats
                    // a screenshot tool never emits — skip those.
                    if foreground::clipboard_image_is_copied() {
                        // Skipped, but last_hash is set so we don't re-check it
                        // every 500ms until the clipboard changes again.
                        continue;
                    }
                    if let Err(e) = handle_new_image(&app, &conn, &paths.base_dir, &ocr_tx, &img, &hash)
                    {
                        eprintln!("[clipboard] handle image: {e}");
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

fn handle_new_image(
    app: &AppHandle,
    conn: &rusqlite::Connection,
    base_dir: &PathBuf,
    ocr_tx: &Sender<OcrJob>,
    img: &arboard::ImageData,
    hash: &str,
) -> Result<(), String> {
    // Already stored (e.g. re-copied an old screenshot)? Skip.
    if db::hash_exists(conn, hash).map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }

    let ts = Utc::now().timestamp_millis();
    let saved = image_saver::save_rgba(
        base_dir,
        img.width as u32,
        img.height as u32,
        &img.bytes,
        ts,
    )?;

    let source_app = foreground::foreground_app();

    let new = db::NewShot {
        file_path: saved.file_path.to_string_lossy().to_string(),
        thumb_path: saved.thumb_path.to_string_lossy().to_string(),
        timestamp: ts,
        source_app: source_app.clone(),
        width: saved.width as i64,
        height: saved.height as i64,
        file_size: saved.file_size as i64,
        hash: hash.to_string(),
    };
    let id = db::insert_screenshot(conn, &new).map_err(|e| e.to_string())?;

    // Push the new card to the UI immediately (OCR still pending).
    let dto = db::ScreenshotDto {
        id,
        file_path: new.file_path.clone(),
        timestamp: ts,
        source_app,
        width: new.width,
        height: new.height,
        file_size: new.file_size,
        ocr_text: String::new(),
        ocr_status: db::OCR_PENDING,
        thumbnail: Some(saved.thumb_data_url),
    };
    let _ = app.emit("new-screenshot", &dto);

    // Queue OCR (worker updates the row + FTS and emits `ocr-updated`).
    let _ = ocr_tx.send(OcrJob {
        id,
        image_path: new.file_path,
    });

    Ok(())
}
