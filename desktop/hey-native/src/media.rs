//! CID -> egui texture cache. Bytes are fetched off the UI thread via
//! `social::content_bytes` and decoded to a `ColorImage` on the worker; the GPU
//! upload (`load_texture`) happens on the UI thread when the `Media` event lands.

use std::collections::HashMap;
use std::sync::mpsc::Sender as EvSender;

use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::engine::Engine;
use crate::state::UiEvent;

enum Slot {
    Loading,
    Ready(TextureHandle),
    Failed,
}

#[derive(Default)]
pub struct MediaCache {
    map: HashMap<String, Slot>,
    order: Vec<String>, // insertion order for a simple LRU-ish cap
}

const CAP: usize = 96;

impl MediaCache {
    /// Return a usable texture for `cid`, kicking off a background fetch on a miss.
    /// Returns `None` while loading or on failure (caller draws a placeholder).
    pub fn texture(
        &mut self,
        cid: &str,
        engine: &Engine,
        ev_tx: &EvSender<UiEvent>,
    ) -> Option<TextureHandle> {
        if cid.is_empty() {
            return None;
        }
        match self.map.get(cid) {
            Some(Slot::Ready(t)) => return Some(t.clone()),
            Some(_) => return None, // Loading or Failed
            None => {}
        }
        // miss -> mark loading + dispatch fetch+decode
        self.map.insert(cid.to_string(), Slot::Loading);
        self.order.push(cid.to_string());
        let cid_owned = cid.to_string();
        let cid_for_fetch = cid.to_string();
        engine.call(
            ev_tx,
            move || async move { hey_mobile_runtime::social::content_bytes(&cid_for_fetch).await },
            move |bytes| UiEvent::Media {
                cid: cid_owned,
                img: decode(&bytes),
            },
        );
        None
    }

    /// Apply a decoded image (UI thread): upload to the GPU and store the handle.
    pub fn apply(&mut self, ctx: &egui::Context, cid: String, img: Result<ColorImage, String>) {
        let slot = match img {
            Ok(ci) => Slot::Ready(ctx.load_texture(&cid, ci, TextureOptions::LINEAR)),
            Err(e) => {
                log::debug!("media decode {cid}: {e}");
                Slot::Failed
            }
        };
        // `cid` is already in `order` from the miss-insert in `texture` (Loading slots
        // are never evicted), so this only overwrites the slot. Guard defensively so a
        // stray `apply` for an unknown cid can't desync `order` from `map`.
        if !self.map.contains_key(&cid) {
            self.order.push(cid.clone());
        }
        self.map.insert(cid, slot);
        self.evict();
    }

    /// LRU eviction. `order` is kept in lockstep with `map` (one entry pushed on
    /// insert, never re-pushed), so `order.len() == map.len()`. While over CAP, find
    /// the OLDEST resolved (Ready/Failed) entry and drop it — skipping `Loading` cids
    /// in place rather than re-pushing them (which used to desync `order` from `map`
    /// and made later `remove(0)` drop the wrong cid). If every over-CAP entry is
    /// still `Loading` (the content-provider EAGAIN flap) there is nothing resolved to
    /// evict yet; we stop this pass rather than grow `order` — the cap is re-checked on
    /// the next `apply`, so the map can't grow without bound once anything resolves.
    fn evict(&mut self) {
        while self.order.len() > CAP {
            // Oldest-first index of a resolved entry.
            let pos = self
                .order
                .iter()
                .position(|cid| !matches!(self.map.get(cid), Some(Slot::Loading)));
            match pos {
                Some(i) => {
                    let old = self.order.remove(i);
                    self.map.remove(&old);
                }
                // All entries still Loading — nothing to drop this pass.
                None => break,
            }
        }
    }
}

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    if bytes.is_empty() {
        return Err("empty".into());
    }
    let dynimg = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let rgba = dynimg.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        rgba.as_raw(),
    ))
}
