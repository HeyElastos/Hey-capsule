//! Per-capsule storage — the patch-0002 route the WASM app prefers:
//! `GET/PUT/DELETE /api/apps/<capsule>/storage/<path>`. Backed by the app's
//! private data dir. hey-core's dispatch_storage probes patch-0002 first and
//! only falls back to the legacy `/api/localhost/*` path on 401/403, so serving
//! this cleanly (404 for a missing file, never 401/403/405) pins the app here.

use std::path::{Path, PathBuf};

pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(dir: &Path) -> Self {
        let root = dir.join("storage");
        let _ = std::fs::create_dir_all(&root);
        Storage { root }
    }

    /// Map `<capsule>/<suffix>` to a file UNDER root, rejecting traversal. BOTH `capsule` and
    /// `suffix` are split on '/' and every component is filtered, so a value like "../identity.json"
    /// (whether it arrives in `capsule` or `suffix`) decomposes to ["..","identity.json"] and the
    /// ".." is dropped — it can never escape `root`. (CVE: passing `capsule` un-split let "../x"
    /// through PathBuf::push and reach the seed file.) A final lexical guard asserts containment.
    fn path(&self, capsule: &str, suffix: &str) -> Option<PathBuf> {
        let mut p = self.root.clone();
        for seg in capsule
            .split('/')
            .chain(suffix.split('/'))
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        {
            p.push(seg);
        }
        // Defense in depth: never return a path outside the storage root.
        if !p.starts_with(&self.root) {
            return None;
        }
        Some(p)
    }

    /// `None` == the file does not exist / path rejected (the caller returns 404).
    pub fn get(&self, capsule: &str, suffix: &str) -> Option<String> {
        std::fs::read_to_string(self.path(capsule, suffix)?).ok()
    }

    pub fn put(&self, capsule: &str, suffix: &str, body: &str) -> Result<(), String> {
        let p = self.path(capsule, suffix).ok_or("invalid path")?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        // ATOMIC write: std::fs::write is truncate-then-write, so a crash or interleaved write mid-
        // way leaves a TORN file — and since these blobs are sealed-at-rest, a torn blob is
        // undecryptable = the WHOLE file (contacts/posts/keys) is lost. Write a sibling .tmp then
        // rename over the target (rename is atomic on the same filesystem). Same-file writes are
        // serialized upstream by storage_lock/contacts_gate, so the fixed .tmp name can't collide.
        let tmp = p.with_extension("heytmp");
        std::fs::write(&tmp, body).map_err(|e| format!("write: {e}"))?;
        std::fs::rename(&tmp, &p).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("rename: {e}")
        })
    }

    pub fn delete(&self, capsule: &str, suffix: &str) -> Result<(), String> {
        let p = self.path(capsule, suffix).ok_or("invalid path")?;
        match std::fs::remove_file(p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove: {e}")),
        }
    }
}
