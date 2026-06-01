//! Best-effort foreground window / app name (spec §4.2). Native on Windows so
//! we don't pull a cross-platform C dependency; other platforms return "Unknown"
//! for now (the field is purely informational).

/// Title of the current foreground window, or `None` if it can't be determined.
#[cfg(windows)]
pub fn foreground_app() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowTextLengthW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        // Prefer the process executable name (e.g. "WeChat.exe"); fall back to
        // the window title text.
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            if let Ok(handle) =
                OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            {
                let mut buf = [0u16; MAX_PATH as usize];
                let len = GetModuleBaseNameW(handle, None, &mut buf);
                let _ = CloseHandle(handle);
                if len > 0 {
                    let name = String::from_utf16_lossy(&buf[..len as usize]);
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
        }

        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(hwnd, &mut buf);
            if read > 0 {
                return Some(String::from_utf16_lossy(&buf[..read as usize]));
            }
        }
        None
    }
}

#[cfg(not(windows))]
pub fn foreground_app() -> Option<String> {
    None
}

/// Whether the image currently on the clipboard looks like it was *copied*
/// (from a file or a web page) rather than produced by a screenshot tool.
///
/// Heuristic: copying an image *file* or a *web image* puts a locator format on
/// the clipboard that names where the image came from:
///   - `CF_HDROP`  — a file was copied in Explorer (carries its path)
///   - `HTML Format` — copied from a web page / rich editor
///   - `FileNameW` / `FileName` — legacy single-file drag formats
/// Only those "where did this come from" formats are used. We deliberately do
/// NOT key off `PNG`/`image/png`: screenshot tools (微信/QQ/Snipping Tool) also
/// drop a PNG blob next to the bitmap, so treating PNG as a copy-signal made
/// real screenshots from those apps get skipped.
#[cfg(windows)]
pub fn clipboard_image_is_copied() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };

    // Standard format ids (winuser.h).
    const CF_HDROP: u32 = 15;

    // Returns true if a named format is currently on the clipboard.
    let named_present = |name: &str| -> bool {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let id = RegisterClipboardFormatW(PCWSTR(wide.as_ptr()));
            id != 0 && IsClipboardFormatAvailable(id).is_ok()
        }
    };

    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_ok() {
            return true; // a file was copied
        }
    }
    // Locator formats a screenshot tool never emits.
    named_present("HTML Format") || named_present("FileNameW") || named_present("FileName")
}

#[cfg(not(windows))]
pub fn clipboard_image_is_copied() -> bool {
    false
}
