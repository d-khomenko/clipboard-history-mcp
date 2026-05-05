use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;

pub struct CaptureContext {
    pub front_app: Option<String>,
    pub window_title: Option<String>,
}

pub fn capture(with_window_title: bool) -> CaptureContext {
    let front_app = autoreleasepool(|_| {
        let ws = NSWorkspace::sharedWorkspace();
        ws.frontmostApplication()
            .and_then(|app| app.localizedName().map(|n| n.to_string()))
    });
    let window_title = if with_window_title {
        capture_window_title(&front_app)
    } else {
        None
    };
    CaptureContext {
        front_app,
        window_title,
    }
}

fn capture_window_title(_front_app: &Option<String>) -> Option<String> {
    // Real Accessibility wiring deferred to v0.3.1.
    // The accessibility crate needs the running app's PID, which requires a deeper
    // objc2 traversal through NSRunningApplication.processIdentifier. Shipping the
    // simpler path first; window titles will always be None in v0.3.0-alpha.0.
    None
}
