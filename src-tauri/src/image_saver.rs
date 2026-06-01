//! Decode clipboard image data, persist the full PNG, and build a thumbnail
//! (spec §4.2 + §7 "thumbnail generation").

use base64_lite::encode_data_url;
use chrono::{Datelike, Local};
use image::{imageops::FilterType, ImageFormat, RgbaImage};
use std::path::{Path, PathBuf};

/// Width of the list thumbnail in pixels. Larger than the spec's 200px so the
/// grid stays crisp on high-DPI displays / wide windows instead of looking
/// pixelated ("mosaic"). Lanczos3 resampling keeps text edges sharp.
const THUMB_WIDTH: u32 = 400;

pub struct SavedImage {
    pub file_path: PathBuf,
    pub thumb_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    /// Thumbnail as a `data:image/png;base64,...` URL for immediate UI display.
    pub thumb_data_url: String,
}

/// Save raw RGBA pixels (as handed over by `arboard`) under
/// `<base>/Screenshots/YYYY/MM/<timestamp>-<uuid>.png` plus a thumbnail.
pub fn save_rgba(
    base_dir: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    timestamp_ms: i64,
) -> Result<SavedImage, String> {
    let img = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "clipboard pixel buffer size mismatch".to_string())?;

    let now = Local::now();
    let dir = base_dir
        .join("Screenshots")
        .join(format!("{:04}", now.year()))
        .join(format!("{:02}", now.month()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    let stem = format!("{}-{}", timestamp_ms, uuid::Uuid::new_v4());
    let file_path = dir.join(format!("{stem}.png"));
    let thumb_path = dir.join(format!("{stem}.thumb.png"));

    img.save_with_format(&file_path, ImageFormat::Png)
        .map_err(|e| format!("save png: {e}"))?;

    write_thumbnail(&img, &thumb_path)?;

    let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
    let thumb_data_url = thumb_to_data_url(&thumb_path)?;

    Ok(SavedImage {
        file_path,
        thumb_path,
        width,
        height,
        file_size,
        thumb_data_url,
    })
}

/// Scale an image down to THUMB_WIDTH (keeping aspect ratio) and write it as
/// PNG. Uses Lanczos3 so downscaled text stays legible.
fn write_thumbnail(img: &RgbaImage, thumb_path: &Path) -> Result<(), String> {
    let (w, h) = (img.width(), img.height());
    let th = if w > THUMB_WIDTH {
        let nh = ((THUMB_WIDTH as f32 / w as f32) * h as f32).round() as u32;
        image::imageops::resize(img, THUMB_WIDTH, nh.max(1), FilterType::Lanczos3)
    } else {
        img.clone()
    };
    th.save_with_format(thumb_path, ImageFormat::Png)
        .map_err(|e| format!("save thumb: {e}"))
}

/// Regenerate a thumbnail from an existing full-size image file. Used by the
/// one-time startup upgrade that resamples old low-res thumbnails.
pub fn regenerate_thumbnail(file_path: &Path, thumb_path: &Path) -> Result<(), String> {
    let img = image::open(file_path)
        .map_err(|e| format!("open {}: {e}", file_path.display()))?
        .to_rgba8();
    write_thumbnail(&img, thumb_path)
}

/// Read a thumbnail file and return it as a base64 PNG data URL.
pub fn thumb_to_data_url(thumb_path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(thumb_path).map_err(|e| format!("read thumb: {e}"))?;
    Ok(encode_data_url(&bytes, "image/png"))
}

/// Read any image file and return it as a base64 data URL (used for full-size).
pub fn file_to_data_url(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read file: {e}"))?;
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    };
    Ok(encode_data_url(&bytes, mime))
}

/// Minimal base64 (RFC 4648) encoder so we avoid pulling an extra crate just
/// for data URLs. Kept private to this crate.
mod base64_lite {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode_data_url(data: &[u8], mime: &str) -> String {
        let mut out = String::with_capacity(data.len() * 4 / 3 + 32);
        out.push_str("data:");
        out.push_str(mime);
        out.push_str(";base64,");
        encode_into(data, &mut out);
        out
    }

    fn encode_into(data: &[u8], out: &mut String) {
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(n & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
}
