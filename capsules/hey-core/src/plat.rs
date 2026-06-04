//! Platform abstraction for the shared chat engine.
//!
//! `hey-core` runs in two worlds:
//!   * the browser (wasm32) — inside hey-social / hey-chat. Time = `Date.now`,
//!     sleep = `setTimeout`, HTTP = `fetch`, storage = the runtime's HTTP
//!     storage route, logging = `console`.
//!   * a native CLI (`hey-chat-cli`, the cross-runtime DM diagnostic) — Time =
//!     `SystemTime`, sleep = `thread::sleep`, HTTP = a tiny loopback TcpStream
//!     client (the runtime API is plaintext on 127.0.0.1, so no TLS), storage =
//!     local JSON files, logging = `eprintln`.
//!
//! ONLY these leaf primitives diverge. Every byte of protocol logic
//! (dms.rs / outbox.rs / the `peer`/`identity`/`content` provider wrappers)
//! is shared and identical across both, so the CLI exercises the EXACT invite/
//! handshake code path the apps run — which is the whole point: trace where a
//! real cross-runtime invite goes wrong without a browser.

#[cfg(target_arch = "wasm32")]
mod imp {
    pub fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }

    pub async fn sleep_ms(ms: i32) {
        let win = web_sys::window().expect("no window");
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }

    pub fn warn(s: &str) {
        web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(s));
    }

    pub fn debug(s: &str) {
        web_sys::console::debug_1(&wasm_bindgen::JsValue::from_str(s));
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    thread_local! {
        static BASE: RefCell<String> = RefCell::new("http://127.0.0.1:3000".to_string());
        static BEARER: RefCell<String> = RefCell::new(String::new());
        static STORE: RefCell<PathBuf> = RefCell::new(PathBuf::from("/tmp/hey-cli"));
    }

    /// Configure the native runtime endpoint, bearer token, and storage root.
    /// Called once from the CLI's `main` before any engine call.
    pub fn set_base(b: &str) {
        BASE.with(|x| *x.borrow_mut() = b.trim_end_matches('/').to_string());
    }
    pub fn set_bearer(b: &str) {
        BEARER.with(|x| *x.borrow_mut() = b.to_string());
    }
    pub fn set_store(dir: &str) {
        STORE.with(|x| *x.borrow_mut() = PathBuf::from(dir));
    }
    pub fn base_url() -> String {
        BASE.with(|x| x.borrow().clone())
    }
    fn bearer() -> String {
        BEARER.with(|x| x.borrow().clone())
    }
    fn store_root() -> PathBuf {
        STORE.with(|x| x.borrow().clone())
    }

    pub fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    pub async fn sleep_ms(ms: i32) {
        if ms > 0 {
            std::thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    pub fn warn(s: &str) {
        eprintln!("[warn] {s}");
    }

    pub fn debug(s: &str) {
        if std::env::var("HEY_DEBUG").is_ok() {
            eprintln!("[debug] {s}");
        }
    }

    // ── Loopback HTTP/1.1 client (plaintext, 127.0.0.1 only) ─────────────
    //
    // Everything the CLI talks to is the LOCAL runtime, so no TLS. We send
    // `Connection: close` and read to EOF, decoding `Transfer-Encoding:
    // chunked` if present (hyper sometimes chunks). Returns (status, body).

    fn parse_url(url: &str) -> Result<(String, u16, String), String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("only http:// supported (loopback): {url}"))?;
        let slash = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..slash];
        let path = if slash < rest.len() { &rest[slash..] } else { "/" };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| "bad port")?),
            None => (authority.to_string(), 80),
        };
        Ok((host, port, path.to_string()))
    }

    fn decode_body(raw: &[u8]) -> Result<(u16, String), String> {
        // Split headers / body on the first CRLFCRLF.
        let sep = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("malformed HTTP response (no header terminator)")?;
        let head = String::from_utf8_lossy(&raw[..sep]);
        let body_bytes = &raw[sep + 4..];
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("bad status line: {status_line}"))?;
        let chunked = lines.any(|l| {
            let l = l.to_ascii_lowercase();
            l.starts_with("transfer-encoding:") && l.contains("chunked")
        });
        let body = if chunked {
            dechunk(body_bytes)
        } else {
            String::from_utf8_lossy(body_bytes).to_string()
        };
        Ok((status, body))
    }

    fn dechunk(mut b: &[u8]) -> String {
        let mut out = Vec::new();
        loop {
            let Some(nl) = b.windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let size_str = String::from_utf8_lossy(&b[..nl]);
            let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
            if size == 0 {
                break;
            }
            let start = nl + 2;
            let end = (start + size).min(b.len());
            out.extend_from_slice(&b[start..end]);
            // Skip the chunk + its trailing CRLF.
            b = if end + 2 <= b.len() { &b[end + 2..] } else { &[] };
        }
        String::from_utf8_lossy(&out).to_string()
    }

    pub fn http(method: &str, url: &str, body: Option<&str>) -> Result<(u16, String), String> {
        let (host, port, path) = parse_url(url)?;
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("connect {host}:{port}: {e}"))?;
        // 90s: a content/fetch for a freshly-pinned CID can block on bitswap
        // discovery longer than 25s on the first hit (the browser uses fetch()
        // with no such cap, so this only affects the native CLI diagnostic —
        // a short cap made it false-report EAGAIN where the app would wait+win).
        stream.set_read_timeout(Some(Duration::from_secs(90))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(90))).ok();
        let b = body.unwrap_or("");
        let tok = bearer();
        let mut req = String::new();
        req.push_str(&format!("{method} {path} HTTP/1.1\r\n"));
        req.push_str(&format!("Host: {host}\r\n"));
        req.push_str("Connection: close\r\n");
        req.push_str("Content-Type: application/json\r\n");
        if !tok.is_empty() {
            req.push_str(&format!("Authorization: Bearer {tok}\r\n"));
        }
        req.push_str(&format!("Content-Length: {}\r\n\r\n", b.len()));
        req.push_str(b);
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        stream.flush().ok();
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| format!("read: {e}"))?;
        decode_body(&raw)
    }

    // ── File-backed storage + KV ─────────────────────────────────────────
    //
    // Mirrors the runtime's per-capsule storage namespace as a directory tree
    // under the configured store root. `suffix` is e.g. "HeyChat/dm/outbox.json".

    fn safe_path(suffix: &str) -> PathBuf {
        let mut p = store_root();
        for seg in suffix.split('/').filter(|s| !s.is_empty() && *s != "." && *s != "..") {
            p.push(seg);
        }
        p
    }

    /// Read a stored JSON document. `None` == the file does not exist (404).
    pub fn file_read(suffix: &str) -> Option<String> {
        std::fs::read_to_string(safe_path(suffix)).ok()
    }

    pub fn file_write(suffix: &str, content: &str) -> Result<(), String> {
        let p = safe_path(suffix);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
        }
        std::fs::write(&p, content).map_err(|e| format!("write {p:?}: {e}"))
    }

    pub fn file_remove(suffix: &str) -> Result<(), String> {
        let p = safe_path(suffix);
        match std::fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove {p:?}: {e}")),
        }
    }

    /// Key/value store (the session record). Backed by `<store>/kv/<key>.json`.
    pub fn kv_get(key: &str) -> Option<String> {
        file_read(&format!("kv/{key}.json"))
    }
    pub fn kv_set(key: &str, val: &str) {
        let _ = file_write(&format!("kv/{key}.json"), val);
    }
    pub fn kv_del(key: &str) {
        let _ = file_remove(&format!("kv/{key}.json"));
    }
}

pub use imp::*;
