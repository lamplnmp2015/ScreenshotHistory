//! SQLite storage + FTS5 full-text search over OCR content (spec §4.4).
//!
//! We deviate slightly from the spec's trigger-based FTS wiring: the FTS row is
//! kept in sync manually (empty on insert, filled when OCR completes) and uses
//! the `trigram` tokenizer so substring search works for CJK text too.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

/// OCR lifecycle for a screenshot row.
pub const OCR_PENDING: i64 = 0;
pub const OCR_DONE: i64 = 1;
pub const OCR_FAILED: i64 = 2;

/// A row as stored in the `screenshots` table (no thumbnail bytes).
#[derive(Debug, Clone)]
pub struct ScreenshotRow {
    pub id: i64,
    pub file_path: String,
    pub thumb_path: String,
    pub timestamp: i64,
    pub source_app: Option<String>,
    pub width: i64,
    pub height: i64,
    pub file_size: i64,
    pub ocr_text: String,
    pub ocr_status: i64,
}

/// What gets sent to the frontend (adds a base64 thumbnail data URL).
#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotDto {
    pub id: i64,
    pub file_path: String,
    pub timestamp: i64,
    pub source_app: Option<String>,
    pub width: i64,
    pub height: i64,
    pub file_size: i64,
    pub ocr_text: String,
    pub ocr_status: i64,
    pub thumbnail: Option<String>,
}

/// Fields needed to insert a freshly captured screenshot.
pub struct NewShot {
    pub file_path: String,
    pub thumb_path: String,
    pub timestamp: i64,
    pub source_app: Option<String>,
    pub width: i64,
    pub height: i64,
    pub file_size: i64,
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total: i64,
    pub ocr_pending: i64,
    pub total_bytes: i64,
}

/// Open a connection with the pragmas needed for safe multi-thread use.
/// The clipboard monitor, the OCR worker and the command handlers each open
/// their own connection to the same file; WAL + a busy timeout let them share.
pub fn open(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// Create the schema if it does not exist.
pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS screenshots (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path   TEXT NOT NULL,
            thumb_path  TEXT NOT NULL DEFAULT '',
            timestamp   INTEGER NOT NULL,
            source_app  TEXT,
            width       INTEGER NOT NULL DEFAULT 0,
            height      INTEGER NOT NULL DEFAULT 0,
            file_size   INTEGER NOT NULL DEFAULT 0,
            hash        TEXT NOT NULL DEFAULT '',
            ocr_text    TEXT NOT NULL DEFAULT '',
            ocr_status  INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_screenshots_ts ON screenshots(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_screenshots_hash ON screenshots(hash);

        -- Full-text index over OCR content. rowid mirrors screenshots.id.
        CREATE VIRTUAL TABLE IF NOT EXISTS screenshots_fts USING fts5(
            content,
            tokenize = 'trigram'
        );

        -- Small key/value store for one-time migrations / app state.
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
}

/// Read a value from the `meta` key/value store.
pub fn meta_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
        r.get::<_, String>(0)
    })
    .optional()
}

/// Write (upsert) a value into the `meta` key/value store.
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// (id, file_path, thumb_path) for every screenshot — used by the thumbnail
/// regeneration upgrade.
pub fn all_image_paths(conn: &Connection) -> rusqlite::Result<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare("SELECT id, file_path, thumb_path FROM screenshots")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// True if a screenshot with this content hash already exists (dedupe).
pub fn hash_exists(conn: &Connection, hash: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM screenshots WHERE hash = ?1 LIMIT 1",
        params![hash],
        |r| r.get::<_, i64>(0),
    )
    .optional()
}

/// Insert a new screenshot (OCR pending) and an empty FTS row. Returns the id.
pub fn insert_screenshot(conn: &Connection, s: &NewShot) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO screenshots
            (file_path, thumb_path, timestamp, source_app, width, height, file_size, hash, ocr_text, ocr_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '', ?9)",
        params![
            s.file_path,
            s.thumb_path,
            s.timestamp,
            s.source_app,
            s.width,
            s.height,
            s.file_size,
            s.hash,
            OCR_PENDING
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO screenshots_fts(rowid, content) VALUES (?1, '')",
        params![id],
    )?;
    Ok(id)
}

/// Store OCR result for a row and refresh its FTS entry.
pub fn set_ocr(conn: &Connection, id: i64, text: &str, status: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE screenshots SET ocr_text = ?1, ocr_status = ?2 WHERE id = ?3",
        params![text, status, id],
    )?;
    // Replace the FTS row (delete + insert keeps rowid aligned with the id).
    conn.execute("DELETE FROM screenshots_fts WHERE rowid = ?1", params![id])?;
    conn.execute(
        "INSERT INTO screenshots_fts(rowid, content) VALUES (?1, ?2)",
        params![id, text],
    )?;
    Ok(())
}

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<ScreenshotRow> {
    Ok(ScreenshotRow {
        id: r.get("id")?,
        file_path: r.get("file_path")?,
        thumb_path: r.get("thumb_path")?,
        timestamp: r.get("timestamp")?,
        source_app: r.get("source_app")?,
        width: r.get("width")?,
        height: r.get("height")?,
        file_size: r.get("file_size")?,
        ocr_text: r.get("ocr_text")?,
        ocr_status: r.get("ocr_status")?,
    })
}

/// One page of history, newest first.
pub fn get_page(conn: &Connection, offset: i64, limit: i64) -> rusqlite::Result<Vec<ScreenshotRow>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM screenshots ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt
        .query_map(params![limit, offset], row_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Full-text search over OCR content + the source app name.
///
/// OCR content uses the FTS5 trigram index for keywords of length >= 3.
/// The source-app name is NOT in the FTS index, so it is always matched with
/// LIKE — that's why typing a full app name now matches (previously only the
/// short-keyword LIKE branch searched `source_app`).
pub fn search(conn: &Connection, keyword: &str, limit: i64) -> rusqlite::Result<Vec<ScreenshotRow>> {
    let kw = keyword.trim();
    if kw.is_empty() {
        return Ok(Vec::new());
    }
    // Escape LIKE wildcards; backslash first so the escape char itself is safe.
    let like = format!(
        "%{}%",
        kw.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );

    if kw.chars().count() >= 3 {
        // Quote the term so punctuation/CJK is treated literally by FTS5.
        let phrase = format!("\"{}\"", kw.replace('"', "\"\""));
        let mut stmt = conn.prepare(
            "SELECT * FROM screenshots
             WHERE id IN (SELECT rowid FROM screenshots_fts WHERE screenshots_fts MATCH ?1)
                OR source_app LIKE ?2 ESCAPE '\\'
             ORDER BY timestamp DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![phrase, like, limit], row_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    } else {
        // Trigram needs >= 3 chars; fall back to LIKE on both fields.
        let mut stmt = conn.prepare(
            "SELECT * FROM screenshots
             WHERE ocr_text LIKE ?1 ESCAPE '\\' OR source_app LIKE ?1 ESCAPE '\\'
             ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![like, limit], row_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

pub fn get_one(conn: &Connection, id: i64) -> rusqlite::Result<Option<ScreenshotRow>> {
    conn.query_row("SELECT * FROM screenshots WHERE id = ?1", params![id], row_from)
        .optional()
}

/// Delete a row (and its FTS entry). Returns the file + thumb paths to unlink.
pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<Option<(String, String)>> {
    let paths = conn
        .query_row(
            "SELECT file_path, thumb_path FROM screenshots WHERE id = ?1",
            params![id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    if paths.is_some() {
        conn.execute("DELETE FROM screenshots WHERE id = ?1", params![id])?;
        conn.execute("DELETE FROM screenshots_fts WHERE rowid = ?1", params![id])?;
    }
    Ok(paths)
}

pub fn stats(conn: &Connection) -> rusqlite::Result<Stats> {
    conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN ocr_status = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(file_size), 0)
         FROM screenshots",
        [],
        |r| {
            Ok(Stats {
                total: r.get(0)?,
                ocr_pending: r.get(1)?,
                total_bytes: r.get(2)?,
            })
        },
    )
}
