use anyhow::{anyhow, Result};
use arboard::Clipboard;
use std::sync::Mutex;

static CLIPBOARD: once_cell::sync::Lazy<Mutex<Clipboard>> = once_cell::sync::Lazy::new(|| {
    Mutex::new(Clipboard::new().expect("init system clipboard"))
});

pub fn read_clipboard() -> Result<String> {
    let mut cb = CLIPBOARD.lock().map_err(|_| anyhow!("clipboard mutex poisoned"))?;
    match cb.get_text() {
        Ok(s) => Ok(s),
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(anyhow!("clipboard read: {}", e)),
    }
}

pub fn write_clipboard(text: &str) -> Result<()> {
    let mut cb = CLIPBOARD.lock().map_err(|_| anyhow!("clipboard mutex poisoned"))?;
    cb.set_text(text.to_string()).map_err(|e| anyhow!("clipboard write: {}", e))?;
    Ok(())
}

/// Always false in v4. macOS NSPasteboard transient/concealed type detection
/// was removed when we switched to arboard; see CLIPBOARD_IGNORE_APPS in
/// install.rs for the compensation mechanism.
pub fn is_transient() -> bool {
    false
}

/// Best-effort change counter for change-detection. arboard does not expose
/// the underlying changeCount; we hash-compare the text instead in the
/// watcher tick. This function exists for backward API compatibility and
/// just returns the current Unix epoch ms — the watcher does not actually
/// rely on monotonicity.
pub fn change_count() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires real clipboard
    fn round_trip() {
        write_clipboard("hello pasteboard").unwrap();
        let got = read_clipboard().unwrap();
        assert_eq!(got, "hello pasteboard");
    }
}
