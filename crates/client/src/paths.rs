use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn install_dir() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "executable has no parent dir"))
}

pub fn history_dir() -> io::Result<PathBuf> {
    Ok(install_dir()?.join(".history"))
}

pub fn history_file() -> io::Result<PathBuf> {
    Ok(history_dir()?.join("history.json"))
}

pub fn lists_dir() -> io::Result<PathBuf> {
    Ok(install_dir()?.join("lists"))
}

pub fn ensure_history_dir() -> io::Result<PathBuf> {
    let dir = history_dir()?;
    let existed = dir.exists();
    fs::create_dir_all(&dir)?;
    if !existed {
        mark_hidden(&dir);
    }
    Ok(dir)
}

// On Linux/macOS the leading "." already hides the directory. On Windows
// the dot prefix has no effect — we need to set FILE_ATTRIBUTE_HIDDEN.
#[cfg(windows)]
fn mark_hidden(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // Best-effort: if this fails (permissions, unusual filesystem), the
    // directory is still usable — we just won't have the hidden flag set.
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn mark_hidden(_path: &Path) {}

/// Returns sorted `.txt` filenames (not full paths) in `lists/`.
/// Err if `lists/` is missing. Ok(empty vec) if it exists but has no `.txt` files —
/// caller decides how to surface that.
pub fn list_available_lists() -> io::Result<Vec<String>> {
    let dir = lists_dir()?;
    let entries = fs::read_dir(&dir)?;
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("txt") {
                p.file_name().and_then(|s| s.to_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    Ok(out)
}

pub fn word_file_path(filename: &str) -> io::Result<PathBuf> {
    Ok(lists_dir()?.join(filename))
}
