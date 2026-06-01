//! Tauri commands exposed to the frontend (spec §4.5 / §5).

use crate::{db, image_saver, AppState};
use std::path::Path;
use tauri::State;

/// Attach a base64 thumbnail to a DB row for the frontend.
fn to_dto(row: db::ScreenshotRow) -> db::ScreenshotDto {
    let thumbnail = if row.thumb_path.is_empty() {
        None
    } else {
        image_saver::thumb_to_data_url(Path::new(&row.thumb_path)).ok()
    };
    db::ScreenshotDto {
        id: row.id,
        file_path: row.file_path,
        timestamp: row.timestamp,
        source_app: row.source_app,
        width: row.width,
        height: row.height,
        file_size: row.file_size,
        ocr_text: row.ocr_text,
        ocr_status: row.ocr_status,
        thumbnail,
    }
}

#[tauri::command]
pub fn get_history_page(
    state: State<AppState>,
    offset: i64,
    limit: i64,
) -> Result<Vec<db::ScreenshotDto>, String> {
    let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
    let rows = db::get_page(&conn, offset, limit.clamp(1, 200)).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(to_dto).collect())
}

#[tauri::command]
pub fn search_by_text(
    state: State<AppState>,
    keyword: String,
    limit: i64,
) -> Result<Vec<db::ScreenshotDto>, String> {
    let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
    let rows = db::search(&conn, &keyword, limit.clamp(1, 500)).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(to_dto).collect())
}

/// Full-resolution image as a data URL (spec §4.5 preview).
#[tauri::command]
pub fn get_image(state: State<AppState>, id: i64) -> Result<Option<String>, String> {
    let path = {
        let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
        match db::get_one(&conn, id).map_err(|e| e.to_string())? {
            Some(row) => row.file_path,
            None => return Ok(None),
        }
    };
    let p = Path::new(&path);
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(image_saver::file_to_data_url(p)?))
}

/// Remove a screenshot from the history. The DB record (and its FTS entry) is
/// always deleted; the image + thumbnail files on disk are only removed when
/// `delete_file` is true. Default from the UI is record-only so the original
/// screenshot file is kept.
#[tauri::command]
pub fn delete_screenshot(
    state: State<AppState>,
    id: i64,
    delete_file: bool,
) -> Result<(), String> {
    let paths = {
        let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
        db::delete(&conn, id).map_err(|e| e.to_string())?
    };
    if delete_file {
        if let Some((file_path, thumb_path)) = paths {
            let _ = std::fs::remove_file(&file_path);
            if !thumb_path.is_empty() {
                let _ = std::fs::remove_file(&thumb_path);
            }
        }
    }
    Ok(())
}

/// Reveal a screenshot (or the root folder) in the OS file manager.
#[tauri::command]
pub fn open_image_folder(state: State<AppState>, id: Option<i64>) -> Result<(), String> {
    let target = match id {
        Some(id) => {
            let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
            db::get_one(&conn, id)
                .map_err(|e| e.to_string())?
                .map(|r| r.file_path)
        }
        None => None,
    };

    match target {
        Some(file) if Path::new(&file).exists() => reveal_in_explorer(&file),
        _ => open_dir(&state.base_dir.join("Screenshots").to_string_lossy()),
    }
}

#[tauri::command]
pub fn get_stats(state: State<AppState>) -> Result<db::Stats, String> {
    let conn = state.conn.lock().map_err(|_| "db lock poisoned")?;
    db::stats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ocr_available(state: State<AppState>) -> bool {
    state.ocr_available
}

// --- OS file-manager helpers ------------------------------------------------

#[cfg(windows)]
fn reveal_in_explorer(file: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(file)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn reveal_in_explorer(file: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-R", file])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn reveal_in_explorer(file: &str) -> Result<(), String> {
    // No portable "reveal" on Linux; open the containing directory.
    let dir = Path::new(file)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());
    open_dir(&dir)
}

#[cfg(windows)]
fn open_dir(dir: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn open_dir(dir: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn open_dir(dir: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
