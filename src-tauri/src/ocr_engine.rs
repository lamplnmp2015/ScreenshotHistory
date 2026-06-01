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

    // `tesseract <img> stdout -l <lang>` prints recognised text to stdout.
    // `--psm 4` (single column of variable-size text) suits screenshots far
    // better than the default psm 3, which garbles Chinese on short/sparse
    // captures. Verified on mixed CN/EN test images.
    let mut cmd = command_for(bin);
    cmd.arg(image_path)
        .arg("stdout")
        .arg("-l")
        .arg(lang)
        .arg("--psm")
        .arg("4");
    if let Some(td) = tessdata_dir() {
        cmd.arg("--tessdata-dir").arg(td);
    }

    let output = cmd.output().map_err(|e| format!("spawn tesseract: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "tesseract failed ({}): {}",
            lang,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
