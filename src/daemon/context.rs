use serde::Serialize;

#[derive(Serialize)]
pub struct CaptureContext {
    pub front_app: Option<String>,
    pub window_title: Option<String>,
}

pub fn capture(with_window_title: bool) -> CaptureContext {
    let front_app = capture_frontmost_app();
    let window_title = if with_window_title { capture_window_title() } else { None };
    CaptureContext { front_app, window_title }
}

fn capture_frontmost_app() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.app_name).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

#[cfg(target_os = "linux")]
fn capture_window_title() -> Option<String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        // Wayland window title support deferred to v0.4.x (portal API)
        return None;
    }
    capture_window_title_x11().or_else(|| capture_window_title_via_active_win())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

fn capture_window_title_via_active_win() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.title).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "linux")]
fn capture_window_title_x11() -> Option<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];

    let net_active = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()?.reply().ok()?.atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").ok()?.reply().ok()?.atom;
    let utf8 = conn.intern_atom(false, b"UTF8_STRING").ok()?.reply().ok()?.atom;

    let active_reply = conn
        .get_property(false, screen.root, net_active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let active_win = u32::from_ne_bytes(active_reply.value.get(..4)?.try_into().ok()?);
    if active_win == 0 {
        return None;
    }

    let title_reply = conn
        .get_property(false, active_win, net_wm_name, utf8, 0, 1024)
        .ok()?
        .reply()
        .ok()?;

    let title = String::from_utf8(title_reply.value).ok()?;
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}
