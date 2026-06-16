//! Desktop A/V capture + playback for 1:1 calls.
//!
//! Voice: a `cpal` mic-capture stream resamples device-native PCM → 8 kHz mono
//! i16 and feeds `hey_mobile_runtime::voice_send`; a `cpal` playback stream drains
//! `voice_recv` and upsamples to the device rate (zero-filling underruns). Video
//! (nokhwa camera → openh264 → `video_send_frame`; `video_recv_frame` → decode →
//! egui texture) layers on after voice is proven.
//!
//! WIP — being built in the voice/video milestone. Empty for now so the module
//! compiles while the call signaling + UI land.
