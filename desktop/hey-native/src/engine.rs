//! The async bridge between egui (sync, immediate-mode) and the runtime's
//! `social::*` (async, thread-local-bound).
//!
//! hey-core's `plat` BASE/STORE, the capsule ctx, and its CID cache are all
//! THREAD-LOCAL, and `plat::http()` is a blocking sync request. So we mirror the
//! Android JNI discipline exactly: a small pool of dedicated OS threads, each with
//! the thread-locals installed ONCE, each owning a `current_thread` tokio runtime.
//! A UI action ships a Send closure to a worker; the worker builds the future and
//! `block_on`s it locally, so the future NEVER crosses a thread boundary (no `Send`
//! bound on the future, no thread-local loss).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender as EvSender;
use std::sync::Arc;

use crossbeam_channel::{unbounded, Sender};

use crate::state::UiEvent;

/// A unit of work handed to a worker. The worker passes in its own runtime so the
/// future is created and driven entirely on the worker thread.
type Job = Box<dyn FnOnce(&tokio::runtime::Runtime) + Send + 'static>;

#[derive(Clone)]
pub struct Engine {
    tx: Sender<Job>,
    ctx: egui::Context,
    inflight: Arc<AtomicU32>,
    pub store: String,
}

impl Engine {
    pub fn new(port: u16, store: String, ctx: egui::Context, workers: usize) -> Self {
        let (tx, rx) = unbounded::<Job>();
        for i in 0..workers.max(1) {
            Self::spawn_worker(i, port, store.clone(), rx.clone());
        }
        Engine {
            tx,
            ctx,
            inflight: Arc::new(AtomicU32::new(0)),
            store,
        }
    }

    /// Spawn one worker thread. A worker installs hey-core's thread-locals once,
    /// owns a `current_thread` runtime, and drains the job channel forever. The job
    /// body itself is panic-isolated (see `call`), so a panicking op can never kill
    /// the worker — but if the recv loop ever exits (channel closed, or an unforeseen
    /// panic OUTSIDE the isolated body), the thread re-spawns itself defensively so
    /// the pool can never be silently drained.
    fn spawn_worker(i: usize, port: u16, store: String, rx: crossbeam_channel::Receiver<Job>) {
        std::thread::Builder::new()
            .name(format!("hey-engine-{i}"))
            .spawn(move || {
                // Install hey-core thread-locals once for this worker (== ensure_plat).
                hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
                hey_core::plat::set_store(&store);
                hey_mobile_runtime::social::install_ctx();
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("engine current_thread runtime");
                // Materialise a did:key session so signing ops work immediately.
                let _ = rt.block_on(hey_mobile_runtime::social::ensure_session());
                while let Ok(job) = rx.recv() {
                    job(&rt);
                }
                // The channel is only dropped on app teardown; reaching here while the
                // app lives means the loop was broken unexpectedly. Re-spawn so the
                // pool self-heals instead of permanently losing a worker.
                log::error!("hey-engine-{i} recv loop exited; re-spawning worker");
                Self::spawn_worker(i, port, store, rx);
            })
            .expect("spawn engine worker");
    }

    /// Number of dispatched-but-not-finished calls (drives the spinner + repaint).
    pub fn inflight(&self) -> u32 {
        self.inflight.load(Ordering::Relaxed)
    }

    /// Dispatch an async `social::*` call to a worker.
    ///
    /// `make_fut` builds the future ON the worker (so the thread-locals are valid);
    /// `to_event` maps its output to a [`UiEvent`] that is sent back to the UI and a
    /// repaint is requested. Neither the future nor its output need be `Send` — only
    /// the two closures and the resulting `UiEvent` (which always is).
    pub fn call<T, Fut, F, M>(&self, ev_tx: &EvSender<UiEvent>, make_fut: F, to_event: M)
    where
        Fut: std::future::Future<Output = T>,
        F: FnOnce() -> Fut + Send + 'static,
        M: FnOnce(T) -> UiEvent + Send + 'static,
    {
        let tx = ev_tx.clone();
        let ctx = self.ctx.clone();
        let inflight = self.inflight.clone();
        inflight.fetch_add(1, Ordering::Relaxed);
        let job: Job = Box::new(move |rt| {
            // Drop-guard: decrement inflight + request a repaint when this job ends,
            // by ANY path (normal return, early return, or a caught panic). Created
            // BEFORE the call so the counter can never leak even if the work panics —
            // a leaked inflight pins the app to a permanent 120ms busy-repaint and
            // wedges inflight-gated spinners forever.
            let _guard = InflightGuard { inflight: &inflight, ctx: &ctx };
            // Panic-isolate the unit of work. A panic inside the future, `block_on`,
            // or `to_event` is caught here and turned into a UiEvent::Error instead of
            // unwinding out of the worker's recv loop (which would permanently kill
            // the OS thread and, with only a few workers, drain the whole pool into a
            // silent app-wide hang). `AssertUnwindSafe` is sound here: a caught panic
            // discards `out`/the future, and the only shared state (`tx`, the inflight
            // counter) is either consumed or atomic.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let out = rt.block_on(make_fut());
                to_event(out)
            }));
            match result {
                Ok(ev) => {
                    let _ = tx.send(ev);
                }
                Err(e) => {
                    let msg = panic_message(&e);
                    log::error!("engine job panicked: {msg}");
                    let _ = tx.send(UiEvent::Error(format!("Operation failed: {msg}")));
                }
            }
            // `_guard` drops here → inflight.fetch_sub + ctx.request_repaint.
        });
        let _ = self.tx.send(job);
    }
}

/// Decrements the inflight counter and pokes a repaint on drop, so a job can never
/// leak the counter regardless of how it returns (including a caught panic).
struct InflightGuard<'a> {
    inflight: &'a Arc<AtomicU32>,
    ctx: &'a egui::Context,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
        self.ctx.request_repaint();
    }
}

/// Best-effort human string out of a caught panic payload.
fn panic_message(e: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
