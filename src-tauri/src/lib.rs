//! Screenshot History — Tauri 2 application entry point.
//!
//! Wires together: clipboard monitor thread -> image saver -> SQLite -> OCR
//! worker thread, plus the system tray and a global toggle hotkey.

mod clipboard_monitor;
mod commands;
mod db;
mod foreground;
mod image_saver;
mod ocr_engine;
mod ocr_worker;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Filesystem locations, shared (cloned) with the worker threads.
#[derive(Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
    pub db_path: PathBuf,
}

/// State available to command handlers via `State<AppState>`.
pub struct AppState {
    pub conn: Mutex<rusqlite::Connection>,
    pub base_dir: PathBuf,
    pub ocr_available: bool,
}

const TOGGLE_HOTKEY: &str = "CommandOrControl+Shift+H";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Only one shortcut is registered, so any press toggles.
                    if event.state() == ShortcutState::Pressed {
                        toggle_main(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle();

            // 1. Resolve storage paths: <Pictures>/ScreenshotHistory, db inside.
            let base_dir = handle
                .path()
                .picture_dir()
                .unwrap_or_else(|_| handle.path().app_data_dir().unwrap())
                .join("ScreenshotHistory");
            std::fs::create_dir_all(&base_dir)?;
            let db_path = base_dir.join("history.db");
            let paths = AppPaths {
                base_dir: base_dir.clone(),
                db_path: db_path.clone(),
            };

            // 2. Init database (command connection lives in managed state).
            let conn = db::open(&db_path)?;
            db::init(&conn)?;

            // 2b. One-time thumbnail upgrade: older builds wrote 200px thumbs
            // that look pixelated when scaled up. Regenerate them at the new
            // resolution in the background so startup stays responsive.
            maybe_upgrade_thumbnails(&db_path);

            // 3. Detect OCR engine once at startup. First tell the OCR module
            // where bundled resources live so it can find the shipped engine +
            // models (falls back to a system install / PATH if absent).
            if let Ok(res_dir) = handle.path().resource_dir() {
                ocr_engine::set_resource_dir(res_dir);
            }
            let ocr_available = ocr_engine::is_available();
            if !ocr_available {
                eprintln!("[startup] tesseract not found — OCR disabled, capture still works");
            }

            app.manage(AppState {
                conn: Mutex::new(conn),
                base_dir: base_dir.clone(),
                ocr_available,
            });

            // 4. Spawn worker threads: OCR (consumer) + clipboard (producer).
            let (ocr_tx, ocr_rx) = std::sync::mpsc::channel::<clipboard_monitor::OcrJob>();
            ocr_worker::start(handle.clone(), paths.clone(), ocr_rx, ocr_available);
            clipboard_monitor::start(handle.clone(), paths.clone(), ocr_tx);

            // 5. System tray.
            tray::build(handle)?;

            // 6. Global hotkey to toggle the main window (spec §4.6).
            if let Err(e) = handle.global_shortcut().register(TOGGLE_HOTKEY) {
                eprintln!("[startup] register hotkey failed: {e}");
            }

            // 7. Give the window a high-res icon. tao hands the OS a single
            // bitmap for the taskbar/title-bar icon; if that bitmap is small
            // (32px) the OS upscales it on 150%/200% displays and it looks
            // blurry. Setting a 512px source lets the OS downscale instead,
            // which stays crisp at every taskbar size.
            if let Some(win) = handle.get_webview_window("main") {
                match tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")) {
                    Ok(img) => {
                        if let Err(e) = win.set_icon(img) {
                            eprintln!("[startup] set window icon failed: {e}");
                        }
                    }
                    Err(e) => eprintln!("[startup] decode window icon failed: {e}"),
                }
            }

            Ok(())
        })
        // Closing the window hides it to the tray instead of quitting.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history_page,
            commands::search_by_text,
            commands::get_image,
            commands::delete_screenshot,
            commands::open_image_folder,
            commands::get_stats,
            commands::ocr_available,
            commands::reocr_screenshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Screenshot History");
}

/// Current thumbnail scheme version. Bump when the thumbnail size/filter
/// changes so existing thumbnails get regenerated once on next launch.
const THUMB_VERSION: &str = "2";

/// If the stored thumbnail version is behind, regenerate every thumbnail from
/// its full-size source on a background thread, then record the new version.
/// Best-effort: failures on individual images are logged and skipped.
fn maybe_upgrade_thumbnails(db_path: &PathBuf) {
    let db_path = db_path.clone();
    std::thread::spawn(move || {
        let conn = match db::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[thumb-upgrade] open db: {e}");
                return;
            }
        };
        let current = db::meta_get(&conn, "thumb_version").ok().flatten();
        if current.as_deref() == Some(THUMB_VERSION) {
            return; // already up to date
        }
        let rows = match db::all_image_paths(&conn) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[thumb-upgrade] list images: {e}");
                return;
            }
        };
        let (mut ok, mut failed) = (0u32, 0u32);
        for (_id, file_path, thumb_path) in rows {
            let (file, thumb) = (PathBuf::from(&file_path), PathBuf::from(&thumb_path));
            if thumb_path.is_empty() || !file.exists() {
                continue;
            }
            match image_saver::regenerate_thumbnail(&file, &thumb) {
                Ok(()) => ok += 1,
                Err(e) => {
                    failed += 1;
                    eprintln!("[thumb-upgrade] {file_path}: {e}");
                }
            }
        }
        if let Err(e) = db::meta_set(&conn, "thumb_version", THUMB_VERSION) {
            eprintln!("[thumb-upgrade] record version: {e}");
        }
        eprintln!("[thumb-upgrade] regenerated {ok} thumbnail(s), {failed} failed");
    });
}

/// Show the main window if hidden, hide it if currently focused/visible.
fn toggle_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        match win.is_visible() {
            Ok(true) => {
                let _ = win.hide();
            }
            _ => {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }
    }
}
