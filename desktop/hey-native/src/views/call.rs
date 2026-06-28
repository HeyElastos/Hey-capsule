//! The in-call overlay — a full-screen frosted panel over every tab, shown
//! whenever `app.state.call` is not Idle. Three faces:
//!
//!   * Outgoing  — "Calling {name}…", Cancel.
//!   * Incoming  — "{name} is calling…", Decline / Accept (green).
//!   * Active    — the live call: remote video (or a big avatar for voice), a
//!                 picture-in-picture local self-view, a running mm:ss timer, and
//!                 the control bar (mute / camera / hang-up).
//!
//! Video frames come from `app.call_media()` — the decode thread fills `remote`,
//! the capture thread fills `local`; we upload the latest of each to a texture
//! each frame (keyed by a per-call id so a new call gets a fresh texture). H.264
//! capture/encode/decode is real (nokhwa + openh264); a voice call just leaves
//! both slots empty and shows avatars.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions};

use crate::app::App;
use crate::icons;
use crate::state::CallState;
use crate::theme::{Theme, GOLD, GOLD2, NAVY};

/// Hang-up / decline red.
const RED: Color32 = Color32::from_rgb(0xE2, 0x3D, 0x3D);
/// Accept green.
const GREEN: Color32 = Color32::from_rgb(0x2E, 0xB8, 0x5C);

pub fn ui(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if matches!(app.state.call, CallState::Idle) {
        return;
    }
    // Keep repainting while a call is up so the timer ticks + new video frames show.
    ctx.request_repaint_after(std::time::Duration::from_millis(120));

    let call = app.state.call.clone();
    let (name, video, is_active, is_incoming, is_outgoing) = match &call {
        CallState::Outgoing { name, video, .. } => (name.clone(), *video, false, false, true),
        CallState::Incoming { name, video, .. } => (name.clone(), *video, false, true, false),
        CallState::Active { name, video, .. } => (name.clone(), *video, true, false, false),
        CallState::Idle => return,
    };

    // Full-screen dark scrim + central card.
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("call-scrim"),
    ));
    painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(8, 8, 12, 244));

    // Decide actions out here so we don't hold an App borrow across the closure.
    let mut do_accept = false;
    let mut do_decline = false;
    let mut do_hangup = false;
    let mut do_mute = false;
    let mut do_cam = false;

    let muted = app.state.call_muted;
    let cam_off = app.state.call_cam_off;
    let elapsed = app.call_elapsed();

    // Upload latest video textures (Active only).
    let (remote_tex, local_tex) = if is_active {
        (
            upload_frame(app, ctx, "call-remote", true),
            upload_frame(app, ctx, "call-local", false),
        )
    } else {
        (None, None)
    };

    egui::Area::new(egui::Id::new("call-overlay"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            let card_w = 460.0_f32.min(screen.width() - 40.0);
            ui.set_width(card_w);
            ui.vertical_centered(|ui| {
                // ── remote stage (video) or avatar (voice / pre-connect) ──
                let stage_w = card_w;
                let stage_h = if video { card_w * 0.62 } else { 220.0 };
                let (stage_rect, _) =
                    ui.allocate_exact_size(egui::vec2(stage_w, stage_h), Sense::hover());
                draw_stage(
                    ui,
                    theme,
                    stage_rect,
                    &name,
                    video && is_active,
                    remote_tex.as_ref(),
                );

                // PiP self-view (Active video only) — bottom-right of the stage.
                if is_active && video {
                    let pip_w = stage_w * 0.28;
                    let pip_h = pip_w * 0.75;
                    let pip = Rect::from_min_size(
                        egui::pos2(
                            stage_rect.right() - pip_w - 10.0,
                            stage_rect.bottom() - pip_h - 10.0,
                        ),
                        egui::vec2(pip_w, pip_h),
                    );
                    draw_pip(ui, theme, pip, local_tex.as_ref(), cam_off);
                }

                ui.add_space(16.0);
                // ── name + status line ──
                // The overlay always sits on a dark scrim, so the name is near-white
                // in BOTH themes (the scrim is theme-independent).
                ui.label(
                    RichText::new(&name)
                        .size(24.0)
                        .family(icons::display())
                        .color(Color32::from_rgb(0xF2, 0xF2, 0xF5)),
                );
                ui.add_space(4.0);
                let status = if is_outgoing {
                    if video { "Video calling…".to_string() } else { "Calling…".to_string() }
                } else if is_incoming {
                    if video { "Incoming video call".to_string() } else { "Incoming call".to_string() }
                } else {
                    // Active: timer (or "Connecting…" until media links).
                    match elapsed {
                        Some(s) => fmt_timer(s),
                        None => "Connecting…".to_string(),
                    }
                };
                ui.label(RichText::new(status).size(15.0).color(Color32::from_gray(170)));

                ui.add_space(26.0);

                // ── controls ──
                ui.horizontal(|ui| {
                    // Centre the control row.
                    let n_ctrls = if is_incoming { 2 } else if is_active && video { 3 } else { 2 };
                    let btn = 64.0;
                    let gap = 22.0;
                    let row_w = n_ctrls as f32 * btn + (n_ctrls as f32 - 1.0) * gap;
                    let pad = ((card_w - row_w) / 2.0).max(0.0);
                    ui.add_space(pad);

                    if is_incoming {
                        if round_btn(ui, icons::CALL_END, RED, btn).clicked() {
                            do_decline = true;
                        }
                        ui.add_space(gap);
                        if round_btn(ui, icons::CALL, GREEN, btn).clicked() {
                            do_accept = true;
                        }
                    } else {
                        // Active or Outgoing: mute, (camera), hang-up.
                        let mic_icon = if muted { icons::MIC_OFF } else { icons::MIC };
                        let mic_fill = if muted { Color32::from_gray(210) } else { Color32::from_gray(64) };
                        let mic_ink = if muted { NAVY } else { Color32::WHITE };
                        if round_btn_ink(ui, mic_icon, mic_fill, mic_ink, btn).clicked() {
                            do_mute = true;
                        }
                        ui.add_space(gap);
                        if is_active && video {
                            let cam_icon = if cam_off { icons::VIDEOCAM_OFF } else { icons::VIDEOCAM };
                            let cam_fill = if cam_off { Color32::from_gray(210) } else { Color32::from_gray(64) };
                            let cam_ink = if cam_off { NAVY } else { Color32::WHITE };
                            if round_btn_ink(ui, cam_icon, cam_fill, cam_ink, btn).clicked() {
                                do_cam = true;
                            }
                            ui.add_space(gap);
                        }
                        if round_btn(ui, icons::CALL_END, RED, btn).clicked() {
                            do_hangup = true;
                        }
                    }
                });
                ui.add_space(8.0);
            });
        });

    // Esc hangs up / declines (a universal "get me out").
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if is_incoming {
            do_decline = true;
        } else {
            do_hangup = true;
        }
    }

    // Apply (one mutable App borrow, after the UI closure).
    if do_accept {
        app.accept_call();
    } else if do_decline {
        app.decline_call();
    } else if do_hangup {
        app.hangup();
    }
    if do_mute {
        app.toggle_mute();
    }
    if do_cam {
        app.toggle_cam();
    }
}

/// mm:ss (or h:mm:ss past an hour) timer string.
fn fmt_timer(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Draw the main stage: a remote video frame if we have one, else a large gold
/// gradient avatar (voice call, or video that hasn't connected yet).
fn draw_stage(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: Rect,
    name: &str,
    expect_video: bool,
    tex: Option<&TextureHandle>,
) {
    let painter = ui.painter();
    painter.rect_filled(rect, 18.0, Color32::from_gray(20));
    painter.rect_stroke(rect, 18.0, Stroke::new(1.0, theme.glass_border));

    if let Some(tex) = tex {
        // Cover-fit the frame into the rounded stage.
        let uv = cover_uv(tex.size_vec2(), rect.size());
        painter.image(tex.id(), rect, uv, Color32::WHITE);
        return;
    }

    // No frame: big avatar centred.
    let c = rect.center();
    let r = (rect.height() * 0.26).min(78.0);
    super::gradient_circle(painter, c - egui::vec2(0.0, 6.0), r, GOLD2, GOLD);
    let glyph = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    painter.text(
        c - egui::vec2(0.0, 6.0),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(r * 0.9, icons::semibold()),
        NAVY,
    );
    if expect_video {
        painter.text(
            egui::pos2(c.x, rect.bottom() - 22.0),
            Align2::CENTER_CENTER,
            "Connecting video…",
            FontId::proportional(13.0),
            Color32::from_gray(150),
        );
    }
}

/// Draw the picture-in-picture local self-view.
fn draw_pip(ui: &mut egui::Ui, theme: &Theme, rect: Rect, tex: Option<&TextureHandle>, cam_off: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, Color32::from_gray(28));
    painter.rect_stroke(rect, 10.0, Stroke::new(1.5, theme.glass_border));
    if cam_off {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            icons::VIDEOCAM_OFF,
            FontId::proportional(rect.height() * 0.32),
            Color32::from_gray(130),
        );
        return;
    }
    match tex {
        Some(tex) => {
            let uv = cover_uv(tex.size_vec2(), rect.size());
            // Mirror the self-view (selfie) horizontally for a natural mirror feel.
            let uv = Rect::from_min_max(
                egui::pos2(uv.max.x, uv.min.y),
                egui::pos2(uv.min.x, uv.max.y),
            );
            painter.image(tex.id(), rect, uv, Color32::WHITE);
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                icons::PERSON,
                FontId::proportional(rect.height() * 0.34),
                Color32::from_gray(110),
            );
        }
    }
}

/// Compute a center-cover UV rect so an arbitrary-aspect frame fills `dst` with no
/// stretch (crops the overflowing axis).
fn cover_uv(tex: egui::Vec2, dst: egui::Vec2) -> Rect {
    if tex.x <= 0.0 || tex.y <= 0.0 || dst.x <= 0.0 || dst.y <= 0.0 {
        return Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    }
    let tex_ar = tex.x / tex.y;
    let dst_ar = dst.x / dst.y;
    if tex_ar > dst_ar {
        // texture wider → crop sides
        let w = dst_ar / tex_ar;
        let off = (1.0 - w) / 2.0;
        Rect::from_min_max(egui::pos2(off, 0.0), egui::pos2(off + w, 1.0))
    } else {
        let h = tex_ar / dst_ar;
        let off = (1.0 - h) / 2.0;
        Rect::from_min_max(egui::pos2(0.0, off), egui::pos2(1.0, off + h))
    }
}

/// Pull the latest RGB frame from the live call media and upload it to a texture.
/// `remote=true` reads the decode slot; false reads the local-preview slot. Returns
/// None when there is no media or no frame yet.
fn upload_frame(app: &App, ctx: &egui::Context, key: &str, remote: bool) -> Option<TextureHandle> {
    let media = app.call_media()?;
    let slot = if remote { &media.remote } else { &media.local };
    let (w, h, rgb) = slot.peek()?;
    if w == 0 || h == 0 || rgb.len() < w * h * 3 {
        return None;
    }
    let img = egui::ColorImage::from_rgb([w, h], &rgb);
    Some(ctx.load_texture(key, img, TextureOptions::LINEAR))
}

// ── small round control buttons ───────────────────────────────────────────────

/// A solid round control button with white ink (hang-up / accept).
fn round_btn(ui: &mut egui::Ui, glyph: &str, fill: Color32, size: f32) -> egui::Response {
    round_btn_ink(ui, glyph, fill, Color32::WHITE, size)
}

/// A solid round control button with an explicit ink color.
fn round_btn_ink(ui: &mut egui::Ui, glyph: &str, fill: Color32, ink: Color32, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let press = ui.ctx().animate_bool_with_time(resp.id, down, 0.07);
    let r = size / 2.0 * (1.0 - 0.05 * press);
    let f = if resp.hovered() && !down { fill.gamma_multiply(1.12) } else { fill };
    ui.painter().circle_filled(rect.center(), r, f);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(size * 0.42),
        ink,
    );
    resp
}
