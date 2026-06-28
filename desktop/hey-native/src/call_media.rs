//! Desktop A/V capture + playback for 1:1 calls.
//!
//! VOICE: a `cpal` input stream captures the default mic at its device-native
//! rate, downsamples to **8 kHz mono i16** (the runtime voice plane's format —
//! see `voice.rs`) and feeds `hey_mobile_runtime::voice_send`; a `cpal` output
//! stream drains `voice_recv` and upsamples the 8 kHz mix to the output device's
//! native rate, zero-filling underruns. Both directions use a tiny inline linear
//! resampler (no `rubato` dep). This is the desktop parity for Android's
//! `VoiceAudio.kt` (AudioRecord / AudioTrack). cpal owns audio I/O on its own
//! real-time threads; the runtime owns the encrypted P2P transport (μ-law over
//! QUIC datagrams on the carrier's `hey/voice/1` ALPN).
//!
//! VIDEO: a capture+encode thread owns a `nokhwa` camera + an `openh264` encoder:
//! grab → RGB → 320×240 → I420 → H.264 → `video_send_frame`. A decode thread owns
//! an `openh264` decoder: `video_recv_frame` → YUV → RGB → the shared `remote`
//! slot the overlay uploads as a texture. The capture thread also publishes its
//! own RGB into `local` for the self-preview. Frames are OPAQUE H.264 to the
//! runtime (it just ships `[u32 len][frame]…` over `hey/video/1`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Runtime voice plane format (must match `voice.rs`): 8 kHz mono, 16-bit LE.
const RT_RATE: u32 = 8_000;

/// Video send size — small enough for real-time encode on a CPU encoder and for a
/// snappy direct link, large enough to read a face. Width/height MUST be even (I420).
const VID_W: usize = 320;
const VID_H: usize = 240;
/// Target H.264 bitrate (bps) — modest, cellular-friendly; the runtime's adaptive
/// dropped()-counter loop would back this off further if we wired it (TODO).
const VID_BITRATE: u32 = 400_000;
/// Capture/encode cadence. ~15 fps is fluid for a 1:1 talking-head and keeps the
/// software encoder comfortable.
const VID_FRAME_MS: u64 = 66;

/// One decoded RGB frame ready for the UI to upload as a texture: (w, h, rgb).
pub type RgbFrame = (usize, usize, Vec<u8>);

/// A single-slot shared frame: producers overwrite, the UI takes the latest.
#[derive(Default, Clone)]
pub struct FrameSlot(Arc<Mutex<Option<RgbFrame>>>);

impl FrameSlot {
    fn put(&self, f: RgbFrame) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(f);
        }
    }
    /// Take the latest frame if a NEW one has arrived since the given generation,
    /// returning it plus the new generation. We key on a monotonically-bumped
    /// counter so the UI only re-uploads a texture when the frame actually changed.
    pub fn peek(&self) -> Option<RgbFrame> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

/// Owns the live cpal streams + the optional video threads for one call. Dropping
/// it stops everything (cpal stops a stream on drop; the video threads watch
/// `running`). Held ON the UI thread (cpal `Stream` is `!Send`).
pub struct CallMedia {
    _in_stream: Option<cpal::Stream>,
    _out_stream: Option<cpal::Stream>,
    muted: Arc<AtomicBool>,
    // ── video ──
    vid_running: Arc<AtomicBool>,
    cam_off: Arc<AtomicBool>,
    /// Latest local-preview frame (self-view). `None` until the camera produces one.
    pub local: FrameSlot,
    /// Latest decoded remote frame. `None` until the peer's video arrives.
    pub remote: FrameSlot,
    /// Set true once the camera opened successfully (so the overlay can show a
    /// "camera unavailable" hint instead of an empty self-view forever).
    pub cam_ok: Arc<AtomicBool>,
}

impl CallMedia {
    /// Open the mic + speaker streams (always) and, for a video call, the camera
    /// capture + remote-decode threads. Audio/camera-device failures degrade
    /// gracefully (logged, half-duplex / no-preview) — a call is never *blocked*
    /// by a device problem.
    pub fn start(video: bool) -> Self {
        let muted = Arc::new(AtomicBool::new(false));
        let host = cpal::default_host();

        let in_stream = match host.default_input_device() {
            Some(dev) => build_input(&dev, muted.clone()),
            None => {
                log::warn!("call: no default input device — capture disabled");
                None
            }
        };
        let out_stream = match host.default_output_device() {
            Some(dev) => build_output(&dev),
            None => {
                log::warn!("call: no default output device — playback disabled");
                None
            }
        };

        let mut media = CallMedia {
            _in_stream: in_stream,
            _out_stream: out_stream,
            muted,
            vid_running: Arc::new(AtomicBool::new(false)),
            cam_off: Arc::new(AtomicBool::new(false)),
            local: FrameSlot::default(),
            remote: FrameSlot::default(),
            cam_ok: Arc::new(AtomicBool::new(false)),
        };
        if video {
            media.start_video();
        }
        media
    }

    /// Mute/unmute the mic. Drives both our capture-side gate and the runtime's
    /// send-side mute, so a remote peer truly hears nothing.
    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Ordering::Relaxed);
        hey_mobile_runtime::voice_set_muted(m);
    }

    /// Camera on/off mid-call (video calls only). Stops emitting frames without
    /// tearing the lane down, mirroring the runtime's `video_set_paused`.
    pub fn set_cam_off(&self, off: bool) {
        self.cam_off.store(off, Ordering::Relaxed);
        hey_mobile_runtime::video_set_paused(off);
    }

    /// Spawn the capture+encode thread and the recv+decode thread.
    fn start_video(&mut self) {
        self.vid_running.store(true, Ordering::Relaxed);
        spawn_capture_encode(
            self.vid_running.clone(),
            self.cam_off.clone(),
            self.cam_ok.clone(),
            self.local.clone(),
        );
        spawn_recv_decode(self.vid_running.clone(), self.remote.clone());
    }
}

impl Drop for CallMedia {
    fn drop(&mut self) {
        // Tell the video threads to exit; the camera/encoder/decoder free as their
        // threads unwind. cpal streams stop on their own drop. The runtime media
        // session itself is stopped by App::stop_media (voice_stop/video_stop).
        self.vid_running.store(false, Ordering::Relaxed);
    }
}

// ── audio: capture ────────────────────────────────────────────────────────────

/// Build + start the mic-capture stream: device-native → 8 kHz mono i16 → `voice_send`.
fn build_input(dev: &cpal::Device, muted: Arc<AtomicBool>) -> Option<cpal::Stream> {
    let config = match dev.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("call: input default config failed: {e}");
            return None;
        }
    };
    let src_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    log::info!("call: mic {channels} ch @ {src_rate} Hz -> 8 kHz mono");

    let err = |e| log::warn!("call: input stream error: {e}");
    let mut resampler = Downsampler::new(src_rate, RT_RATE);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => dev.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                if muted.load(Ordering::Relaxed) {
                    return;
                }
                let mono = to_mono_f32(data, channels);
                send_pcm(&resampler.process(&mono));
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => dev.build_input_stream(
            &config.into(),
            move |data: &[i16], _| {
                if muted.load(Ordering::Relaxed) {
                    return;
                }
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let mono = to_mono_f32(&f, channels);
                send_pcm(&resampler.process(&mono));
            },
            err,
            None,
        ),
        other => {
            log::warn!("call: unsupported input sample format {other:?}");
            return None;
        }
    };
    finish(stream, "input")
}

fn send_pcm(samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    hey_mobile_runtime::voice_send(&pcm);
}

// ── audio: playback ───────────────────────────────────────────────────────────

/// Build + start the playback stream: `voice_recv` (8 kHz mono i16) → upsample →
/// fan out to N output channels, zero-filling underruns.
fn build_output(dev: &cpal::Device) -> Option<cpal::Stream> {
    let config = match dev.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("call: output default config failed: {e}");
            return None;
        }
    };
    let dst_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    log::info!("call: speaker {channels} ch @ {dst_rate} Hz <- 8 kHz mono");

    let err = |e| log::warn!("call: output stream error: {e}");
    let mut upsampler = Upsampler::new(RT_RATE, dst_rate);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => dev.build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| {
                let frames = out.len() / channels.max(1);
                let mono = upsampler.pull(frames);
                fill_out_f32(out, &mono, channels);
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => dev.build_output_stream(
            &config.into(),
            move |out: &mut [i16], _| {
                let ch = channels.max(1);
                let frames = out.len() / ch;
                let mono = upsampler.pull(frames);
                for (frame, &s) in out.chunks_mut(ch).zip(mono.iter()) {
                    let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    for c in frame.iter_mut() {
                        *c = v;
                    }
                }
                for c in out.iter_mut().skip(mono.len() * ch) {
                    *c = 0;
                }
            },
            err,
            None,
        ),
        other => {
            log::warn!("call: unsupported output sample format {other:?}");
            return None;
        }
    };
    finish(stream, "output")
}

fn finish(stream: Result<cpal::Stream, cpal::BuildStreamError>, which: &str) -> Option<cpal::Stream> {
    match stream {
        Ok(s) => match s.play() {
            Ok(()) => Some(s),
            Err(e) => {
                log::warn!("call: {which} stream play failed: {e}");
                None
            }
        },
        Err(e) => {
            log::warn!("call: build {which} stream failed: {e}");
            None
        }
    }
}

fn to_mono_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let inv = 1.0 / channels as f32;
    data.chunks_exact(channels)
        .map(|f| f.iter().sum::<f32>() * inv)
        .collect()
}

fn fill_out_f32(out: &mut [f32], mono: &[f32], channels: usize) {
    let ch = channels.max(1);
    for (frame, &s) in out.chunks_mut(ch).zip(mono.iter()) {
        for c in frame.iter_mut() {
            *c = s;
        }
    }
    for c in out.iter_mut().skip(mono.len() * ch) {
        *c = 0.0;
    }
}

// ── video: capture + encode ───────────────────────────────────────────────────

/// Camera → RGB → 320×240 → I420 → H.264 → `video_send_frame`, ~15 fps. Publishes
/// each captured RGB into `local` for the self-preview. All camera/encoder state
/// lives ON this thread (nokhwa `Camera` is `!Send`; the openh264 `Encoder` holds C
/// state). Exits when `running` clears.
fn spawn_capture_encode(
    running: Arc<AtomicBool>,
    cam_off: Arc<AtomicBool>,
    cam_ok: Arc<AtomicBool>,
    local: FrameSlot,
) {
    std::thread::Builder::new()
        .name("call-video-capture".into())
        .spawn(move || {
            use nokhwa::pixel_format::RgbFormat;
            use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
            use nokhwa::Camera;

            let fmt = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut cam = match Camera::new(CameraIndex::Index(0), fmt) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("call: open camera failed: {e}");
                    return;
                }
            };
            if let Err(e) = cam.open_stream() {
                log::warn!("call: camera open_stream failed: {e}");
                return;
            }
            cam_ok.store(true, Ordering::Relaxed);

            let cfg = openh264::encoder::EncoderConfig::new()
                .set_bitrate_bps(VID_BITRATE)
                .max_frame_rate(1000.0 / VID_FRAME_MS as f32)
                .enable_skip_frame(true);
            let api = openh264::OpenH264API::from_source();
            let mut encoder = match openh264::encoder::Encoder::with_api_config(api, cfg) {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("call: openh264 encoder init failed: {e}");
                    return;
                }
            };

            let frame_dur = std::time::Duration::from_millis(VID_FRAME_MS);
            while running.load(Ordering::Relaxed) {
                let t0 = std::time::Instant::now();
                let buffer = match cam.frame() {
                    Ok(b) => b,
                    Err(_) => {
                        std::thread::sleep(frame_dur);
                        continue;
                    }
                };
                let src = match buffer.decode_image::<RgbFormat>() {
                    Ok(img) => img,
                    Err(_) => continue,
                };
                let (sw, sh) = (src.width() as usize, src.height() as usize);
                let rgb = scale_rgb(src.as_raw(), sw, sh, VID_W, VID_H);

                // Self-preview always reflects the live camera (even if paused for
                // sending, the user still sees themselves).
                local.put((VID_W, VID_H, rgb.clone()));

                if !cam_off.load(Ordering::Relaxed) {
                    let yuv = openh264::formats::YUVBuffer::from_rgb8_source(
                        openh264::formats::RgbSliceU8::new(&rgb, (VID_W, VID_H)),
                    );
                    match encoder.encode(&yuv) {
                        Ok(bitstream) => {
                            let bytes = bitstream.to_vec();
                            if !bytes.is_empty() {
                                hey_mobile_runtime::video_send_frame(&bytes);
                            }
                        }
                        Err(e) => log::debug!("call: encode frame failed: {e}"),
                    }
                }

                if let Some(rest) = frame_dur.checked_sub(t0.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
            let _ = cam.stop_stream();
        })
        .ok();
}

/// `video_recv_frame` → openh264 decode → RGB → `remote`. Owns the decoder (C
/// state) on this thread. Exits when `running` clears.
fn spawn_recv_decode(running: Arc<AtomicBool>, remote: FrameSlot) {
    std::thread::Builder::new()
        .name("call-video-decode".into())
        .spawn(move || {
            // `dimensions()` is a YUVSource trait method, not inherent.
            use openh264::formats::YUVSource;
            let api = openh264::OpenH264API::from_source();
            let mut decoder =
                match openh264::decoder::Decoder::with_api_config(api, openh264::decoder::DecoderConfig::new()) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("call: openh264 decoder init failed: {e}");
                        return;
                    }
                };
            let mut rgb: Vec<u8> = Vec::new();
            while running.load(Ordering::Relaxed) {
                let frame = hey_mobile_runtime::video_recv_frame();
                if frame.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(8));
                    continue;
                }
                match decoder.decode(&frame) {
                    Ok(Some(yuv)) => {
                        let (w, h) = yuv.dimensions();
                        rgb.resize(w * h * 3, 0);
                        yuv.write_rgb8(&mut rgb);
                        remote.put((w, h, rgb.clone()));
                    }
                    Ok(None) => {} // need more data (mid-keyframe)
                    Err(e) => log::debug!("call: decode frame failed: {e}"),
                }
            }
        })
        .ok();
}

/// Nearest-neighbour RGB rescale to (dw, dh). Cheap + good enough for a small
/// preview/encode target; the source is whatever resolution the camera gave us.
fn scale_rgb(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    let mut out = vec![0u8; dw * dh * 3];
    if sw == 0 || sh == 0 {
        return out;
    }
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = (sy * sw + sx) * 3;
            let di = (y * dw + x) * 3;
            if si + 2 < src.len() {
                out[di] = src[si];
                out[di + 1] = src[si + 1];
                out[di + 2] = src[si + 2];
            }
        }
    }
    out
}

// ── inline linear resamplers (audio) ──────────────────────────────────────────
// Linear interpolation is what AudioTrack-style speech pipelines use and is
// inaudible for 8 kHz μ-law voice. State persists across callbacks so the
// fractional phase stays continuous (no per-buffer clicks).

/// Device-rate mono → 8 kHz mono.
struct Downsampler {
    ratio: f32,
    pos: f32,
    last: f32,
    primed: bool,
}

impl Downsampler {
    fn new(src: u32, dst: u32) -> Self {
        Downsampler {
            ratio: src as f32 / dst as f32,
            pos: 0.0,
            last: 0.0,
            primed: false,
        }
    }
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        if !self.primed {
            self.last = input[0];
            self.primed = true;
        }
        let n = input.len() as f32;
        let mut out = Vec::with_capacity((n / self.ratio) as usize + 1);
        while self.pos < n {
            let i = self.pos.floor() as isize;
            let frac = self.pos - self.pos.floor();
            let a = if i < 0 { self.last } else { input[i as usize] };
            let b = if (i + 1) < input.len() as isize {
                input[(i + 1) as usize]
            } else {
                input[input.len() - 1]
            };
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        self.pos -= n;
        self.last = input[input.len() - 1];
        out
    }
}

/// 8 kHz mono → device-rate mono, draining the runtime jitter buffer on demand.
struct Upsampler {
    ratio: f32,
    pos: f32,
    src: Vec<f32>,
}

impl Upsampler {
    fn new(src: u32, dst: u32) -> Self {
        Upsampler {
            ratio: src as f32 / dst as f32,
            pos: 0.0,
            src: Vec::new(),
        }
    }
    fn pull(&mut self, frames: usize) -> Vec<f32> {
        let want_src = (frames as f32 * self.ratio).ceil() as usize + 2;
        while self.src.len() < want_src + self.pos.floor() as usize {
            let pcm = hey_mobile_runtime::voice_recv(want_src * 2);
            if pcm.is_empty() {
                break;
            }
            for ch in pcm.chunks_exact(2) {
                self.src
                    .push(i16::from_le_bytes([ch[0], ch[1]]) as f32 / i16::MAX as f32);
            }
        }
        let mut out = Vec::with_capacity(frames);
        for _ in 0..frames {
            let i = self.pos.floor() as usize;
            if i + 1 >= self.src.len() {
                out.push(0.0);
                continue;
            }
            let frac = self.pos - i as f32;
            let a = self.src[i];
            let b = self.src[i + 1];
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        let consumed = self.pos.floor() as usize;
        if consumed > 0 && consumed <= self.src.len() {
            self.src.drain(0..consumed);
            self.pos -= consumed as f32;
        }
        out
    }
}
