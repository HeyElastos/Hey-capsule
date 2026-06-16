//! In-memory ring-buffer logger so the app can show its OWN runtime logs
//! on-device (tap the status pill) — no adb needed. Every `log::*` record is
//! kept in a bounded ring AND forwarded to the platform log (logcat on Android,
//! stderr on host). Exposed over `/api/runtime/logs`.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Metadata, Record};

const CAP: usize = 800;

fn ring() -> &'static Mutex<VecDeque<String>> {
    static RING: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAP)))
}

/// Most-recent-last snapshot of the captured log lines.
pub fn snapshot() -> Vec<String> {
    ring().lock().map(|b| b.iter().cloned().collect()).unwrap_or_default()
}

struct RingLogger;

impl log::Log for RingLogger {
    fn enabled(&self, _m: &Metadata) -> bool {
        true
    }
    fn log(&self, r: &Record) {
        let target = r.target();
        // Keep the in-app view readable: our own logs at any level, but only
        // warnings/errors from dependencies (iroh emits a flood of trace spans
        // via tracing's log bridge). Drop tracing span scaffolding entirely.
        if target.starts_with("tracing") {
            return;
        }
        let ours = target.starts_with("hey_mobile_runtime");
        if !ours && r.level() > Level::Warn {
            return;
        }
        let line = format!("{:<5} {}: {}", r.level(), target, r.args());
        if let Ok(mut b) = ring().lock() {
            if b.len() >= CAP {
                b.pop_front();
            }
            b.push_back(line.clone());
        }
        platform_emit(r.level(), &line);
    }
    fn flush(&self) {}
}

/// Install the ring logger as the global `log` sink. Idempotent: if another
/// logger is already set (e.g. env_logger in a host test) this is a no-op for
/// the global sink, but the ring still won't capture — call this FIRST.
pub fn init(level: LevelFilter) {
    if log::set_boxed_logger(Box::new(RingLogger)).is_ok() {
        log::set_max_level(level);
    }
}

#[cfg(target_os = "android")]
fn platform_emit(level: Level, msg: &str) {
    use std::ffi::CString;
    // android log priorities: 2=V 3=D 4=I 5=W 6=E
    let prio = match level {
        Level::Error => 6,
        Level::Warn => 5,
        Level::Info => 4,
        Level::Debug => 3,
        Level::Trace => 2,
    };
    if let (Ok(tag), Ok(m)) = (CString::new("HeyRuntime"), CString::new(msg)) {
        unsafe {
            android_log_sys::__android_log_write(prio, tag.as_ptr(), m.as_ptr());
        }
    }
}

#[cfg(not(target_os = "android"))]
fn platform_emit(_level: Level, msg: &str) {
    eprintln!("{msg}");
}
