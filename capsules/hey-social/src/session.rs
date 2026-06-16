// Session state — sourced from the shared `hey-core` engine instead of a
// local copy.
//
// hey-social's copy was identical to the engine's except for two hardcoded
// localStorage key constants. The engine reads those keys from `CapsuleCtx`
// (set in main.rs: session_key "hey-social-session", welcomed_key
// "hey-social-welcomed"), so re-exporting is a zero-behavior-change dedup —
// the same `Session` struct, the same keys, no re-login.
//
// All existing `crate::session::*` call sites keep compiling unchanged.
pub use hey_core::session::*;
