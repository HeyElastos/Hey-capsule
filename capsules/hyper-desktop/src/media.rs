//! Browser capture for calls.
//!
//! hey-core has no WebRTC and a capsule must not open iroh/UDP. Local
//! picture and sound come from `getUserMedia`. Remote tiles are either
//! WebCodecs H.264 (AVC / annex-B) or JPEG snapshots, published on the
//! call's Carrier topic after the route has a peer — we do not start live
//! media on an unproved path. Encode and decode happen in this tab.
//!
//! Devices are remembered BY NAME (see `prefs`). An index would silently
//! become a different camera the next time something is unplugged.

use crate::prefs;
use js_sys::{Array, Object, Reflect, Uint8Array};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    ConstrainDomStringParameters, HtmlCanvasElement, HtmlVideoElement, MediaDeviceInfo,
    MediaDeviceKind, MediaStream, MediaStreamConstraints, MediaTrackConstraints,
};

/// Same wall-clock IDR interval as Hyper-Skia (`VID_IDR_MS`). openh264 used
/// to emit one keyframe per call; a late joiner then never configured.
const VID_IDR_MS: i64 = 2_000;
/// Gossip fragment original-wire ceiling: `frag::MAX_FRAGS * CHUNK_BYTES`.
const MAX_FRAG_WIRE: usize = 128 * 3000;
/// Provider HTTP body cap — a single gossip_send must stay under this.
const PROVIDER_BODY_CAP: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub kind: &'static str,
}

/// Two cameras both called "MX Brio" must still be distinguishable.
pub fn disambiguate(devices: &[Device]) -> Vec<Device> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for d in devices {
        *counts.entry(d.label.clone()).or_insert(0) += 1;
    }
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    devices
        .iter()
        .map(|d| {
            let n = counts.get(&d.label).copied().unwrap_or(1);
            if n <= 1 || d.label.is_empty() {
                return d.clone();
            }
            let i = seen.entry(d.label.clone()).or_insert(0);
            *i += 1;
            let tail: String = d.id.chars().rev().take(4).collect::<String>().chars().rev().collect();
            Device {
                label: format!("{} ({tail})", d.label),
                ..d.clone()
            }
        })
        .collect()
}

fn media_devices() -> Option<web_sys::MediaDevices> {
    web_sys::window()?.navigator().media_devices().ok()
}

pub async fn enumerate() -> Vec<Device> {
    let Some(md) = media_devices() else {
        return Vec::new();
    };
    let Ok(p) = md.enumerate_devices() else {
        return Vec::new();
    };
    let Ok(v) = JsFuture::from(p).await else {
        return Vec::new();
    };
    let Ok(arr) = v.dyn_into::<Array>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..arr.length() {
        let Ok(info) = arr.get(i).dyn_into::<MediaDeviceInfo>() else {
            continue;
        };
        let kind = match info.kind() {
            MediaDeviceKind::Videoinput => "camera",
            MediaDeviceKind::Audioinput => "mic",
            MediaDeviceKind::Audiooutput => "speaker",
            _ => continue,
        };
        let label = info.label();
        let id = info.device_id();
        if id.is_empty() {
            continue;
        }
        out.push(Device {
            id,
            label: if label.is_empty() {
                format!("{kind} {i}")
            } else {
                label
            },
            kind,
        });
    }
    disambiguate(&out)
}

fn constrain_exact(id: &str) -> MediaTrackConstraints {
    let mut c = MediaTrackConstraints::new();
    let mut d = ConstrainDomStringParameters::new();
    d.exact(&id.into());
    c.device_id(&d.into());
    c
}

/// Open the camera/mic remembered by name. Empty name = browser default.
pub async fn open_stream(video: bool, audio: bool) -> Result<MediaStream, String> {
    let md = media_devices().ok_or_else(|| "no MediaDevices — this runtime has no camera path".to_string())?;
    let mut cons = MediaStreamConstraints::new();
    if video {
        let want = prefs::call_camera();
        let devices = enumerate().await;
        let cam = devices.iter().find(|d| d.kind == "camera" && (d.label == want || d.id == want));
        if !want.is_empty() {
            if let Some(cam) = cam {
                cons.video(&constrain_exact(&cam.id).into());
            } else {
                cons.video(&true.into());
            }
        } else {
            cons.video(&true.into());
        }
    } else {
        cons.video(&false.into());
    }
    if audio {
        let want = prefs::call_mic();
        let devices = enumerate().await;
        let mic = devices.iter().find(|d| d.kind == "mic" && (d.label == want || d.id == want));
        if !want.is_empty() {
            if let Some(mic) = mic {
                cons.audio(&constrain_exact(&mic.id).into());
            } else {
                cons.audio(&true.into());
            }
        } else {
            cons.audio(&true.into());
        }
    } else {
        cons.audio(&false.into());
    }
    let p = md
        .get_user_media_with_constraints(&cons)
        .map_err(|e| format!("{e:?}"))?;
    let v = JsFuture::from(p).await.map_err(|e| format!("{e:?}"))?;
    v.dyn_into::<MediaStream>().map_err(|_| "not a MediaStream".into())
}

pub fn stop_stream(stream: &MediaStream) {
    let tracks = stream.get_tracks();
    for i in 0..tracks.length() {
        if let Ok(t) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
            t.stop();
        }
    }
}

pub fn attach_local(video_el: &HtmlVideoElement, stream: &MediaStream) {
    video_el.set_src_object(Some(stream));
    video_el.set_muted(true);
    let _ = video_el.play();
}

/// Snapshot the local camera as a JPEG data URL.
///
/// Quality and size follow the room: 1:1 on a proved route stays sharp;
/// a huddle drops so Carrier is not asked to carry six 720p stills.
pub fn snapshot_jpeg(video_el: &HtmlVideoElement, room: usize, direct: bool) -> Option<String> {
    let w = video_el.video_width();
    let h = video_el.video_height();
    if w == 0 || h == 0 {
        return None;
    }
    let (max_h, q) = jpeg_ladder(room, direct);
    let scale = (max_h as f64 / h as f64).min(1.0);
    let dw = ((w as f64) * scale).round().max(2.0) as u32;
    let dh = ((h as f64) * scale).round().max(2.0) as u32;
    let doc = web_sys::window()?.document()?;
    let canvas: HtmlCanvasElement = doc.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(dw);
    canvas.set_height(dh);
    let ctx: web_sys::CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_html_video_element_and_dw_and_dh(video_el, 0.0, 0.0, dw as f64, dh as f64)
        .ok()?;
    canvas.to_data_url_with_type_and_encoder_options("image/jpeg", &q.into()).ok()
}

/// JPEG still ladder (fallback when WebCodecs is missing).
fn jpeg_ladder(room: usize, direct: bool) -> (u32, f64) {
    match (room, direct) {
        (0 | 1 | 2, true) => (720, 0.72),
        (0 | 1 | 2, false) => (540, 0.62),
        (3 | 4, _) => (540, 0.55),
        _ => (360, 0.48),
    }
}

#[derive(Clone, Copy)]
pub struct VideoRung {
    pub w: u32,
    pub h: u32,
    pub bitrate: u32,
    pub name: &'static str,
}

/// Encoder config ladder. Config only — this is not native nokhwa.
/// 1:1 full, 3–4 at 720p, 5+ at 540p. A frame that will not fit gossip
/// steps down a rung instead of opening a second transport.
const VIDEO_RUNGS: &[VideoRung] = &[
    VideoRung { w: 1920, h: 1080, bitrate: 5_000_000, name: "1080p" },
    VideoRung { w: 1280, h: 720, bitrate: 3_000_000, name: "720p" },
    VideoRung { w: 960, h: 540, bitrate: 2_000_000, name: "540p" },
    VideoRung { w: 640, h: 360, bitrate: 800_000, name: "360p" },
];

pub fn video_rung(room: usize, direct: bool, demote: usize) -> VideoRung {
    let cap = match (room, direct) {
        (0 | 1 | 2, true) => 0,
        (0 | 1 | 2, false) => 1,
        (3 | 4, _) => 1,
        _ => 2,
    };
    VIDEO_RUNGS[(cap + demote).min(VIDEO_RUNGS.len() - 1)]
}

fn ctor(name: &str) -> Option<js_sys::Function> {
    let w = web_sys::window()?;
    Reflect::get(w.as_ref(), &JsValue::from_str(name))
        .ok()?
        .dyn_into()
        .ok()
}

/// VideoEncoder + VideoDecoder + VideoFrame + EncodedVideoChunk present.
pub fn webcodecs_available() -> bool {
    ctor("VideoEncoder").is_some()
        && ctor("VideoDecoder").is_some()
        && ctor("VideoFrame").is_some()
        && ctor("EncodedVideoChunk").is_some()
}

pub struct EncodedAvc {
    pub key: bool,
    pub pts: i64,
    pub bytes: Vec<u8>,
}

struct EncoderState {
    encoder: JsValue,
    _keep: Vec<Closure<dyn FnMut(JsValue, JsValue)>>,
    _keep1: Vec<Closure<dyn FnMut(JsValue)>>,
    pending: Rc<RefCell<Vec<EncodedAvc>>>,
    last_err: Rc<RefCell<Option<String>>>,
    w: u32,
    h: u32,
    bitrate: u32,
    last_key_ms: i64,
    demote: usize,
}

thread_local! {
    static ENC: RefCell<Option<EncoderState>> = const { RefCell::new(None) };
    static DECS: RefCell<HashMap<String, DecoderState>> = RefCell::new(HashMap::new());
}

pub fn reset_codecs() {
    ENC.with(|e| {
        if let Some(st) = e.borrow_mut().take() {
            let _ = Reflect::apply(
                &Reflect::get(&st.encoder, &JsValue::from_str("close"))
                    .ok()
                    .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                    .unwrap_or(js_sys::Function::new_no_args("")),
                &st.encoder,
                &Array::new(),
            );
        }
    });
    DECS.with(|d| {
        for (_, st) in d.borrow_mut().drain() {
            let _ = call0(&st.decoder, "close");
        }
    });
}

fn call0(obj: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    let f = Reflect::get(obj, &JsValue::from_str(name))?;
    let f: js_sys::Function = f.dyn_into()?;
    f.call0(obj)
}

fn call1(obj: &JsValue, name: &str, a: &JsValue) -> Result<JsValue, JsValue> {
    let f = Reflect::get(obj, &JsValue::from_str(name))?;
    let f: js_sys::Function = f.dyn_into()?;
    f.call1(obj, a)
}

fn call2(obj: &JsValue, name: &str, a: &JsValue, b: &JsValue) -> Result<JsValue, JsValue> {
    let f = Reflect::get(obj, &JsValue::from_str(name))?;
    let f: js_sys::Function = f.dyn_into()?;
    f.call2(obj, a, b)
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

fn set_prop(obj: &Object, k: &str, v: impl Into<JsValue>) {
    let _ = Reflect::set(obj, &JsValue::from_str(k), &v.into());
}

fn encoder_config(w: u32, h: u32, bitrate: u32) -> Object {
    let cfg = Object::new();
    set_prop(&cfg, "codec", "avc1.42001f");
    set_prop(&cfg, "width", w);
    set_prop(&cfg, "height", h);
    set_prop(&cfg, "bitrate", bitrate);
    set_prop(&cfg, "framerate", 5);
    set_prop(&cfg, "latencyMode", "realtime");
    set_prop(&cfg, "hardwareAcceleration", "prefer-hardware");
    set_prop(&cfg, "avc", {
        let avc = Object::new();
        set_prop(&avc, "format", "annexb");
        avc
    });
    cfg
}

fn even(n: u32) -> u32 {
    (n & !1).max(2)
}

fn scaled(src_w: u32, src_h: u32, rung: VideoRung) -> (u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (even(rung.w), even(rung.h));
    }
    let scale = (rung.w as f64 / src_w as f64)
        .min(rung.h as f64 / src_h as f64)
        .min(1.0);
    (even(((src_w as f64) * scale).round() as u32), even(((src_h as f64) * scale).round() as u32))
}

fn build_encoder(w: u32, h: u32, bitrate: u32, demote: usize) -> Result<EncoderState, String> {
    let ctor = ctor("VideoEncoder").ok_or_else(|| "VideoEncoder missing".to_string())?;
    let pending = Rc::new(RefCell::new(Vec::new()));
    let last_err = Rc::new(RefCell::new(None));
    let pending_cb = pending.clone();
    let output = Closure::wrap(Box::new(move |chunk: JsValue, _meta: JsValue| {
        let typ = Reflect::get(&chunk, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let pts = Reflect::get(&chunk, &JsValue::from_str("timestamp"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as i64;
        let byte_len = Reflect::get(&chunk, &JsValue::from_str("byteLength"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32;
        if byte_len == 0 {
            return;
        }
        let buf = Uint8Array::new_with_length(byte_len);
        let _ = call1(&chunk, "copyTo", buf.as_ref());
        let mut bytes = vec![0u8; byte_len as usize];
        buf.copy_to(&mut bytes);
        pending_cb.borrow_mut().push(EncodedAvc {
            key: typ == "key",
            pts,
            bytes,
        });
    }) as Box<dyn FnMut(JsValue, JsValue)>);
    let err_slot = last_err.clone();
    let error = Closure::wrap(Box::new(move |e: JsValue| {
        *err_slot.borrow_mut() = Some(format!("{e:?}"));
    }) as Box<dyn FnMut(JsValue)>);
    let init = Object::new();
    set_prop(&init, "output", output.as_ref().unchecked_ref::<js_sys::Function>().clone());
    set_prop(&init, "error", error.as_ref().unchecked_ref::<js_sys::Function>().clone());
    let encoder = Reflect::construct(&ctor, &Array::of1(init.as_ref()))
        .map_err(|e| format!("VideoEncoder construct: {e:?}"))?;
    let cfg = encoder_config(w, h, bitrate);
    call1(&encoder, "configure", cfg.as_ref()).map_err(|e| format!("VideoEncoder configure: {e:?}"))?;
    Ok(EncoderState {
        encoder,
        _keep: vec![output],
        _keep1: vec![error],
        pending,
        last_err,
        w,
        h,
        bitrate,
        last_key_ms: 0,
        demote,
    })
}

/// Encode one camera frame as annex-B H.264. `None` means use JPEG fallback.
pub async fn encode_avc(
    video_el: &HtmlVideoElement,
    room: usize,
    direct: bool,
) -> Option<EncodedAvc> {
    if !webcodecs_available() {
        return None;
    }
    let src_w = video_el.video_width();
    let src_h = video_el.video_height();
    if src_w == 0 || src_h == 0 {
        return None;
    }
    let demote = ENC.with(|e| e.borrow().as_ref().map(|s| s.demote).unwrap_or(0));
    let rung = video_rung(room, direct, demote);
    let (w, h) = scaled(src_w, src_h, rung);
    let need_new = ENC.with(|e| {
        e.borrow()
            .as_ref()
            .map(|s| s.w != w || s.h != h || s.bitrate != rung.bitrate)
            .unwrap_or(true)
    });
    if need_new {
        match build_encoder(w, h, rung.bitrate, demote) {
            Ok(st) => ENC.with(|e| *e.borrow_mut() = Some(st)),
            Err(err) => {
                leptos::logging::warn!("avc encoder: {err}");
                return None;
            }
        }
    }
    let now = now_ms();
    let force_key = ENC.with(|e| {
        let mut b = e.borrow_mut();
        let Some(st) = b.as_mut() else { return true };
        if let Some(err) = st.last_err.borrow().clone() {
            leptos::logging::warn!("avc encoder: {err}");
            return false;
        }
        st.last_key_ms == 0 || now - st.last_key_ms >= VID_IDR_MS
    });
    if ENC.with(|e| e.borrow().as_ref().and_then(|s| s.last_err.borrow().clone()).is_some()) {
        ENC.with(|e| *e.borrow_mut() = None);
        return None;
    }
    let vf_ctor = ctor("VideoFrame")?;
    let opts = Object::new();
    set_prop(&opts, "timestamp", (now * 1000) as f64);
    let args = Array::new();
    args.push(video_el.as_ref());
    args.push(opts.as_ref());
    let frame = Reflect::construct(&vf_ctor, &args).ok()?;
    let enc_opts = Object::new();
    set_prop(&enc_opts, "keyFrame", force_key);
    let encode_ok = ENC.with(|e| {
        let b = e.borrow();
        let Some(st) = b.as_ref() else { return false };
        call2(&st.encoder, "encode", &frame, enc_opts.as_ref()).is_ok()
    });
    let _ = call0(&frame, "close");
    if !encode_ok {
        return None;
    }
    gloo_timers::future::TimeoutFuture::new(8).await;
    let chunk = ENC.with(|e| {
        let mut b = e.borrow_mut();
        let Some(st) = b.as_mut() else { return None };
        let mut p = st.pending.borrow_mut();
        let last = p.pop();
        p.clear();
        if last.as_ref().map(|c| c.key).unwrap_or(false) || force_key {
            st.last_key_ms = now;
        }
        last
    })?;
    if !wire_fits_avc(chunk.bytes.len()) {
        ENC.with(|e| {
            if let Some(st) = e.borrow_mut().as_mut() {
                st.demote = (st.demote + 1).min(VIDEO_RUNGS.len() - 1);
                leptos::logging::warn!(
                    "avc frame {} B exceeds gossip/provider cap — demoting to {}",
                    chunk.bytes.len(),
                    video_rung(room, direct, st.demote).name
                );
            }
        });
        return None;
    }
    Some(chunk)
}

fn wire_fits_avc(raw_len: usize) -> bool {
    // JSON + base64 expansion (~4/3) must stay under frag + provider caps.
    let b64 = raw_len.saturating_mul(4).div_ceil(3) + 128;
    b64 <= PROVIDER_BODY_CAP && b64 <= MAX_FRAG_WIRE
}

pub fn wire_too_big(s: &str) -> bool {
    s.len() > PROVIDER_BODY_CAP || s.len() > MAX_FRAG_WIRE
}

struct DecoderState {
    decoder: JsValue,
    _keep: Vec<Closure<dyn FnMut(JsValue, JsValue)>>,
    _keep1: Vec<Closure<dyn FnMut(JsValue)>>,
    frames: Rc<RefCell<Vec<JsValue>>>,
    last_err: Rc<RefCell<Option<String>>>,
}

fn build_decoder() -> Result<DecoderState, String> {
    let ctor = ctor("VideoDecoder").ok_or_else(|| "VideoDecoder missing".to_string())?;
    let frames = Rc::new(RefCell::new(Vec::new()));
    let last_err = Rc::new(RefCell::new(None));
    let frames_cb = frames.clone();
    let output = Closure::wrap(Box::new(move |frame: JsValue, _: JsValue| {
        frames_cb.borrow_mut().push(frame);
    }) as Box<dyn FnMut(JsValue, JsValue)>);
    let err_slot = last_err.clone();
    let error = Closure::wrap(Box::new(move |e: JsValue| {
        *err_slot.borrow_mut() = Some(format!("{e:?}"));
    }) as Box<dyn FnMut(JsValue)>);
    let init = Object::new();
    set_prop(&init, "output", output.as_ref().unchecked_ref::<js_sys::Function>().clone());
    set_prop(&init, "error", error.as_ref().unchecked_ref::<js_sys::Function>().clone());
    let decoder = Reflect::construct(&ctor, &Array::of1(init.as_ref()))
        .map_err(|e| format!("VideoDecoder construct: {e:?}"))?;
    let cfg = Object::new();
    set_prop(&cfg, "codec", "avc1.42001f");
    set_prop(&cfg, "hardwareAcceleration", "prefer-hardware");
    set_prop(&cfg, "optimizeForLatency", true);
    call1(&decoder, "configure", cfg.as_ref()).map_err(|e| format!("VideoDecoder configure: {e:?}"))?;
    Ok(DecoderState {
        decoder,
        _keep: vec![output],
        _keep1: vec![error],
        frames,
        last_err,
    })
}

/// Decode one annex-B chunk to a JPEG data URL for the existing tile `<img>`.
/// Failures are labelled (never silent), matching Skia's decode-fail tile.
pub async fn decode_avc(peer: &str, b64: &str, key: bool, pts: i64) -> Result<String, String> {
    if !webcodecs_available() {
        return Err("VideoDecoder missing — remote H.264 would not decode".into());
    }
    let bytes = b64_decode(b64).ok_or_else(|| "remote H.264 was not valid base64".to_string())?;
    if bytes.is_empty() {
        return Err("remote H.264 chunk was empty".into());
    }
    let need = DECS.with(|d| !d.borrow().contains_key(peer));
    if need {
        let st = build_decoder()?;
        DECS.with(|d| {
            d.borrow_mut().insert(peer.to_string(), st);
        });
    }
    let chunk_ctor = ctor("EncodedVideoChunk").ok_or_else(|| "EncodedVideoChunk missing".to_string())?;
    let init = Object::new();
    set_prop(&init, "type", if key { "key" } else { "delta" });
    set_prop(&init, "timestamp", pts as f64);
    let data = Uint8Array::from(bytes.as_slice());
    set_prop(&init, "data", data);
    let chunk = Reflect::construct(&chunk_ctor, &Array::of1(init.as_ref()))
        .map_err(|e| format!("EncodedVideoChunk: {e:?}"))?;
    DECS.with(|d| {
        let b = d.borrow();
        let st = b.get(peer).ok_or_else(|| "decoder vanished".to_string())?;
        if let Some(err) = st.last_err.borrow().clone() {
            return Err(format!("VideoDecoder: {err}"));
        }
        call1(&st.decoder, "decode", &chunk).map_err(|e| format!("VideoDecoder.decode: {e:?}"))?;
        Ok(())
    })?;
    gloo_timers::future::TimeoutFuture::new(8).await;
    let frame = DECS.with(|d| {
        let b = d.borrow();
        let st = b.get(peer)?;
        if let Some(err) = st.last_err.borrow().clone() {
            leptos::logging::warn!("VideoDecoder: {err}");
        }
        let x = st.frames.borrow_mut().pop();
        x
    });
    let Some(frame) = frame else {
        return Err("remote H.264 would not decode (no picture — waiting for IDR)".into());
    };
    let w = Reflect::get(&frame, &JsValue::from_str("displayWidth"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;
    let h = Reflect::get(&frame, &JsValue::from_str("displayHeight"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u32;
    if w == 0 || h == 0 {
        let _ = call0(&frame, "close");
        return Err("remote H.264 decoded a zero-size picture".into());
    }
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| "no document".to_string())?;
    let canvas: HtmlCanvasElement = doc
        .create_element("canvas")
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "canvas".to_string())?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into().ok())
        .ok_or_else(|| "2d context".to_string())?;
    // drawImage(VideoFrame) is the WebCodecs display path.
    let draw = Reflect::get(ctx.as_ref(), &JsValue::from_str("drawImage"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| "drawImage missing".to_string())?;
    let args = Array::of5(
        &frame,
        &JsValue::from(0),
        &JsValue::from(0),
        &JsValue::from(w),
        &JsValue::from(h),
    );
    draw.apply(ctx.as_ref(), &args)
        .map_err(|e| format!("drawImage: {e:?}"))?;
    let _ = call0(&frame, "close");
    canvas
        .to_data_url_with_type_and_encoder_options("image/jpeg", &0.72.into())
        .map_err(|e| format!("remote frame would not decode ({e:?})"))
}

pub fn b64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for c in bytes.chunks(4) {
        let a = val(c[0])?;
        let b = val(c[1])?;
        let cc = if c[2] == b'=' { 0 } else { val(c[2])? };
        let d = if c[3] == b'=' { 0 } else { val(c[3])? };
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((cc as u32) << 6) | d as u32;
        out.push((n >> 16) as u8);
        if c[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            out.push(n as u8);
        }
    }
    Some(out)
}

pub async fn read_file_bytes(file: &web_sys::File) -> Result<(String, String, Vec<u8>), String> {
    let name = file.name();
    let mime = file.type_();
    let buf = JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| format!("{name}: could not be read ({e:?})"))?;
    let arr = js_sys::Uint8Array::new(&buf);
    let mut bytes = vec![0u8; arr.length() as usize];
    arr.copy_to(&mut bytes);
    if bytes.is_empty() {
        return Err(format!("{name}: could not be read (empty)"));
    }
    Ok((name, if mime.is_empty() { "application/octet-stream".into() } else { mime }, bytes))
}

/// Read every file, or refuse the whole send. An unreadable file must not
/// vanish while the rest go out — that was the desktop bug.
pub async fn read_all_or_none(files: &[web_sys::File]) -> Result<Vec<(String, String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    let mut refused = Vec::new();
    for f in files {
        match read_file_bytes(f).await {
            Ok(v) => out.push(v),
            Err(e) => refused.push(e),
        }
    }
    if let Some(first) = refused.first() {
        return Err(if refused.len() > 1 {
            format!("{first} ({} more like it)", refused.len() - 1)
        } else {
            first.clone()
        });
    }
    Ok(out)
}
