//! OCR via the `tesseract` CLI (spec §2 "方案A"), chosen over the `leptess`
//! binding so the project builds without Leptonica/Tesseract C dev libraries.
//! If `tesseract` cannot be located the app still works — OCR text stays empty
//! and rows are marked failed/skipped.
//!
//! Resolution order for the binary (first that runs `--version` wins, cached):
//!   1. a bundled copy shipped next to the app   (`<exe>/tesseract/tesseract.exe`)
//!   2. `tesseract` on PATH
//!   3. common Windows install dirs (UB-Mannheim default, winget, choco)
//! This is why simply installing Tesseract without adding it to PATH now works.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Cached path to a working `tesseract` binary (or None if none found).
static TESSERACT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
/// Cached tessdata dir to pass via `--tessdata-dir` (None = let tesseract decide).
static TESSDATA_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
/// The app's resource dir, resolved by Tauri at startup. The bundled tesseract
/// lives at `<resource_dir>/tesseract/`. Set once before the first OCR call.
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Record the Tauri resource directory so the bundled engine/models can be
/// found regardless of where the installer placed the app. Call once at
/// startup (before any OCR runs); later calls are ignored.
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dir);
}

/// Candidate locations for the tesseract executable, in priority order.
fn candidate_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();

    // 1. Bundled with the app, located via Tauri's resolved resource dir.
    if let Some(res) = RESOURCE_DIR.get() {
        v.push(res.join("tesseract").join("tesseract.exe"));
        v.push(res.join("tesseract").join("tesseract")); // non-windows
    }

    // 2. Bundled alongside the exe (fallback if resource dir wasn't set):
    //    <exe dir>/tesseract/tesseract.exe
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("tesseract").join("tesseract.exe"));
            v.push(dir.join("tesseract").join("tesseract"));
        }
    }

    // 3. PATH lookup (bare name lets the OS resolve it).
    v.push(PathBuf::from("tesseract"));

    // 4. Common Windows install locations.
    #[cfg(windows)]
    {
        v.push(PathBuf::from(r"C:\Program Files\Tesseract-OCR\tesseract.exe"));
        v.push(PathBuf::from(r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(local).join(r"Programs\Tesseract-OCR\tesseract.exe"));
        }
    }
    v
}

/// Build a `Command` for a given binary path with the no-flash-window flag.
fn command_for(path: &std::path::Path) -> Command {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Resolve (and cache) the tesseract binary by probing each candidate's
/// `--version`. Returns the first that runs successfully.
fn resolve() -> Option<&'static PathBuf> {
    TESSERACT_PATH
        .get_or_init(|| {
            for cand in candidate_paths() {
                let ok = command_for(&cand)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if ok {
                    eprintln!("[ocr] using tesseract at: {}", cand.display());
                    return Some(cand);
                }
            }
            None
        })
        .as_ref()
}

/// Detect whether a usable `tesseract` binary is available anywhere.
pub fn is_available() -> bool {
    resolve().is_some()
}

/// Resolve (and cache) the tessdata directory that contains the language
/// models. Preference:
///   1. a writable app-managed dir under %LOCALAPPDATA% if it has chi_sim
///   2. a `tessdata` folder next to the resolved binary
///   3. None — let tesseract use its built-in default
fn tessdata_dir() -> Option<&'static PathBuf> {
    TESSDATA_DIR
        .get_or_init(|| {
            // App-managed models (where we download chi_sim into).
            if let Some(app) = app_tessdata_dir() {
                if app.join("chi_sim.traineddata").exists() {
                    return Some(app);
                }
            }
            // Bundled models shipped with the app: <resource>/tesseract/tessdata.
            if let Some(res) = RESOURCE_DIR.get() {
                let td = res.join("tesseract").join("tessdata");
                if td.join("chi_sim.traineddata").exists() || td.join("eng.traineddata").exists() {
                    return Some(td);
                }
            }
            // Next to the binary (system install's own tessdata).
            if let Some(bin) = resolve() {
                if let Some(dir) = bin.parent() {
                    let td = dir.join("tessdata");
                    if td.is_dir() {
                        return Some(td);
                    }
                }
            }
            None
        })
        .as_ref()
}

/// `%LOCALAPPDATA%/ScreenshotHistory/tessdata` — a user-writable place to drop
/// language models (notably chi_sim) without touching Program Files.
pub fn app_tessdata_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("ScreenshotHistory").join("tessdata"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Run OCR on an image file. Tries `chi_sim+eng` first (Chinese + English),
/// then falls back to `eng` if the Chinese language pack is missing.
/// Returns recognised text (possibly empty).
pub fn extract_text(image_path: &str) -> Result<String, String> {
    match run_with_lang(image_path, "chi_sim+eng") {
        Ok(t) => Ok(t),
        Err(_) => run_with_lang(image_path, "eng"),
    }
}

fn run_with_lang(image_path: &str, lang: &str) -> Result<String, String> {
    let bin = resolve().ok_or_else(|| "tesseract not found".to_string())?;

    // tesseract with the `tsv` config writes TWO files:
    //   <outputbase>.tsv  — TSV with per-word confidence
    //   <outputbase>.txt  — plain recognised text
    // We use the TSV for confidence-filtered output, but fall back to the
    // plain-text file when TSV filtering produces nothing (e.g. all words
    // dropped, or the TSV file is malformed).
    let tmp_base = std::env::temp_dir().join(format!("sh_ocr_{}", uuid::Uuid::new_v4()));
    let tsv_path = tmp_base.with_extension("tsv");
    let txt_path = tmp_base.with_extension("txt");

    let mut cmd = command_for(bin);
    cmd.arg(image_path)
        .arg(&tmp_base)
        .arg("-l")
        .arg(lang)
        .arg("--psm")
        .arg("4")
        .arg("tsv");
    if let Some(td) = tessdata_dir() {
        cmd.arg("--tessdata-dir").arg(td);
    }

    let output = cmd.output().map_err(|e| format!("spawn tesseract: {e}"))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tsv_path);
        let _ = std::fs::remove_file(&txt_path);
        return Err(format!(
            "tesseract failed ({}): {}",
            lang,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let tsv_text = std::fs::read_to_string(&tsv_path).unwrap_or_default();
    let txt_text = std::fs::read_to_string(&txt_path).unwrap_or_default();

    let _ = std::fs::remove_file(&tsv_path);
    let _ = std::fs::remove_file(&txt_path);

    // Prefer confidence-filtered TSV result; the quality gate inside
    // filter_tsv rejects garbled output from partial / icon-only images.
    if let Some(filtered) = filter_tsv(&tsv_text) {
        eprintln!("[ocr] TSV filtered ({}): {} chars", lang, filtered.len());
        return Ok(filtered);
    }

    // TSV filtering returned None (quality rejected or no word data).
    // Only fall back to plain text when the TSV file had zero word rows,
    // which indicates a genuine parsing / file-creation problem — not a
    // quality rejection.  If tesseract found words but they were low
    // quality, we trust the rejection and don't fall back.
    let has_word_data = tsv_text.lines().any(|l| l.starts_with("5\t"));
    if !has_word_data && !txt_text.is_empty() {
        eprintln!(
            "[ocr] TSV has no word data, falling back to plain text ({}): {} chars",
            lang,
            txt_text.len()
        );
        return Ok(txt_text.trim().to_string());
    }

    // Either quality rejected, or both TSV + plain text are empty.
    eprintln!("[ocr] no usable text ({})", lang);
    Ok(String::new())
}

/// Minimum per-word confidence (0–100) to keep a word. Tuned so icon/graphic
/// hallucinations (usually < this) are dropped while genuine text survives.
const MIN_WORD_CONF: f32 = 55.0;

/// True for CJK ideographs / kana — used to avoid inserting spurious spaces
/// between Chinese characters when rejoining filtered words.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // hiragana + katakana
        0x3400..=0x4DBF |   // CJK ext A
        0x4E00..=0x9FFF |   // CJK unified
        0xF900..=0xFAFF |   // CJK compat
        0xFF00..=0xFFEF     // fullwidth forms
    )
}

/// Parse tesseract TSV, keep words with confidence >= MIN_WORD_CONF, and
/// reconstruct text line by line. Returns `None` when the result is rejected
/// by the quality gate (too few surviving words, too many dropped, or average
/// confidence too low) — this prevents garbled output from partial screenshots
/// or icon-only images from being stored.
fn filter_tsv(tsv: &str) -> Option<String> {
    // TSV columns: level page block par line word left top width height conf text
    let mut lines: Vec<String> = Vec::new();
    let mut cur_key: Option<(u32, u32, u32)> = None; // (block, par, line)
    let mut cur = String::new();

    let mut total_words = 0u32;
    let mut kept_words = 0u32;
    let mut kept_conf_sum = 0f64;

    let mut flush = |buf: &mut String, out: &mut Vec<String>| {
        let t = buf.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        buf.clear();
    };

    for (i, row) in tsv.lines().enumerate() {
        if i == 0 {
            continue; // header
        }
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        // Only word-level rows (level 5) carry text + confidence.
        if cols[0] != "5" {
            continue;
        }
        let conf: f32 = cols[10].parse().unwrap_or(-1.0);
        let word = cols[11].trim();
        if word.is_empty() {
            continue;
        }
        total_words += 1;
        let key = (
            cols[2].parse().unwrap_or(0),
            cols[3].parse().unwrap_or(0),
            cols[4].parse().unwrap_or(0),
        );
        if cur_key != Some(key) {
            flush(&mut cur, &mut lines);
            cur_key = Some(key);
        }
        if conf < MIN_WORD_CONF {
            continue; // drop low-confidence (icon hallucination) words
        }
        kept_words += 1;
        kept_conf_sum += conf as f64;
        // Join: no space between two CJK chars, otherwise a single space.
        if let (Some(prev), Some(next)) = (cur.chars().last(), word.chars().next()) {
            if !(is_cjk(prev) && is_cjk(next)) {
                cur.push(' ');
            }
        }
        cur.push_str(word);
    }
    flush(&mut cur, &mut lines);

    // ---- quality gate ----
    // Partial screenshots and icons produce lots of low-confidence noise that
    // we drop above.  Three checks ensure the *surviving* text is still
    // trustworthy enough to store:
    if kept_words < 3 {
        eprintln!(
            "[ocr] reject: only {} words passed ({} total)",
            kept_words, total_words
        );
        return None;
    }
    let drop_rate = 1.0 - (kept_words as f32 / total_words.max(1) as f32);
    if drop_rate > 0.6 {
        eprintln!(
            "[ocr] reject: {:.0}% words dropped ({}/{} total)",
            drop_rate * 100.0,
            total_words - kept_words,
            total_words
        );
        return None;
    }
    let avg_conf = kept_conf_sum / kept_words as f64;
    if avg_conf < 58.0 {
        eprintln!("[ocr] reject: avg confidence {:.1} < 58", avg_conf);
        return None;
    }

    eprintln!(
        "[ocr] quality ok: {} words, avg conf {:.1}, {:.0}% dropped",
        kept_words, avg_conf, drop_rate * 100.0
    );
    let result = lines.join("\n");
    if result.is_empty() {
        return None;
    }
    Some(result)
}
