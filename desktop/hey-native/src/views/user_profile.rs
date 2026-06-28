//! Full-screen peer-profile overlay (Android `UserProfileScreen`), shown while
//! `app.state.viewed.is_some()`. Header: back/close, avatar(84), nickname or
//! short DID, bio, the full DID (muted, ellipsised). A Follow/Following button +
//! a Message button. Then "Posts (N)" and a 3-column grid of the user's posts.
//!
//! Rendered as a CENTER_CENTER egui Window that fills the viewport, so it floats
//! over whichever tab is active. The opener already dispatched `app.load_user`.

use egui::{Align2, Margin, RichText, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{OpenChat, Tab};
use crate::theme::{Theme, GOLD, NAVY};

use super::avatar;

pub fn ui(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    // Snapshot everything we read so we can freely mutate app.state below.
    let viewed = match app.state.viewed.clone() {
        Some(v) => v,
        None => return,
    };
    let did = viewed.did.clone();
    let prof = viewed.profile.clone();
    let nickname = prof.get("nickname").and_then(Value::as_str).unwrap_or("").to_string();
    let bio = prof.get("bio").and_then(Value::as_str).unwrap_or("").to_string();
    let av = prof.get("avatar").and_then(Value::as_str).unwrap_or("").to_string();
    let following_them = viewed.following_them;
    let posts = viewed.posts.clone();

    let mut close = false;
    let mut do_follow = false;
    let mut do_message = false;
    let mut do_tip = false;

    // ISOLATION: probe whether a private chat with this profile is permitted (chat established, not
    // just a follow). Once per did; feeds chatable_dids. The Message button gates on it below — the
    // engine enforces the send regardless, but this avoids opening a dead, non-sendable chat.
    if app.state.chatable_requested.insert(did.clone()) {
        app.fetch_can_chat(did.clone());
    }

    let screen = ctx.screen_rect();
    egui::Window::new("user_profile")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size(Vec2::new(screen.width(), screen.height()))
        .frame(
            egui::Frame::none()
                .fill(theme.bg2)
                .inner_margin(Margin::same(0.0)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Header bar (full width, padded from the corners)
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        if super::icon_button(ui, theme, icons::ARROW_BACK, 20.0, theme.ink).clicked() {
                            close = true;
                        }
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("Profile")
                                .size(18.0)
                                .family(crate::icons::semibold())
                                .color(theme.ink),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(16.0);
                            if super::icon_button(ui, theme, icons::CLOSE, 20.0, theme.muted).clicked() {
                                close = true;
                            }
                        });
                    });

                    // Body in a centered, max-width column so nothing hugs the edges
                    // and the identity / actions / posts all share one alignment.
                    let max_w = 680.0_f32;
                    let side = ((ui.available_width() - max_w) / 2.0).max(28.0);
                    egui::Frame::none()
                        .inner_margin(Margin { left: side, right: side, top: 14.0, bottom: 0.0 })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            // Identity (centered)
                            ui.vertical_centered(|ui| {
                                avatar(&mut app.media, &app.engine, &app.ev_tx, ui, &av, &did, 96.0);
                                ui.add_space(14.0);
                                let title = if nickname.is_empty() {
                                    crate::state::AppState::short_did(&did)
                                } else {
                                    nickname.clone()
                                };
                                ui.label(
                                    RichText::new(title)
                                        .size(22.0)
                                        .family(crate::icons::semibold())
                                        .color(theme.ink),
                                );
                                if !bio.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(&bio).size(14.0).color(theme.muted));
                                }
                                ui.add_space(6.0);
                                ui.add(
                                    egui::Label::new(RichText::new(&did).size(12.0).color(theme.muted))
                                        .truncate(),
                                );
                            });

                            // Centered action row (Follow + Message, equal width), all
                            // routed through the shared button kit so they match the rest
                            // of the app: Follow = gold primary (dimmed when already
                            // following), Message = plain outline, Tip = tinted gold
                            // secondary.
                            ui.add_space(18.0);
                            ui.vertical_centered(|ui| {
                                ui.scope(|ui| {
                                    ui.set_width(320.0_f32.min(ui.available_width()));
                                    let bw = (ui.available_width() - 10.0) / 2.0;
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(bw, 46.0),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_width(bw);
                                                if following_them {
                                                    // Already following → dimmed-gold, non-actionable.
                                                    super::push_button(
                                                        ui, true, "Following",
                                                        GOLD.gamma_multiply(0.4),
                                                        GOLD.gamma_multiply(0.4),
                                                        NAVY.gamma_multiply(0.7),
                                                    );
                                                } else if super::primary_button(ui, true, "Follow").clicked() {
                                                    do_follow = true;
                                                }
                                            },
                                        );
                                        ui.add_space(10.0);
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(bw, 46.0),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_width(bw);
                                                if super::outline_button(
                                                    ui, theme, true,
                                                    &format!("{}  Message", icons::CHAT_BUBBLE_OUTLINE),
                                                )
                                                .clicked()
                                                {
                                                    do_message = true;
                                                }
                                            },
                                        );
                                    });
                                    // Tip by identity — opens the Tip sheet for this DID.
                                    ui.add_space(10.0);
                                    if super::secondary_button(ui, theme, true, &format!("{}  Tip", icons::PAID)).clicked() {
                                        do_tip = true;
                                    }
                                });
                            });

                            if !viewed.status.is_empty() {
                                ui.add_space(10.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(&viewed.status).size(12.0).color(theme.muted));
                                });
                            }

                            // Posts (left-aligned within the column)
                            ui.add_space(24.0);
                            ui.label(
                                RichText::new(format!("Posts ({})", posts.len()))
                                    .size(16.0)
                                    .family(crate::icons::semibold())
                                    .color(theme.ink),
                            );
                            ui.add_space(12.0);

                            if !viewed.loaded {
                                ui.add_space(20.0);
                                ui.vertical_centered(|ui| ui.spinner());
                            } else if posts.is_empty() {
                                ui.add_space(20.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("No posts yet").size(14.0).color(theme.muted));
                                });
                            } else {
                                posts_grid(app, ui, theme, &posts);
                            }

                            ui.add_space(40.0);
                        });
                });
        });

    // ── apply deferred actions (state no longer borrowed) ─────────────────────
    if do_follow {
        app.follow(did.clone());
        if let Some(vu) = app.state.viewed.as_mut() {
            vu.following_them = true;
        }
    }
    if do_tip {
        let name = if nickname.is_empty() {
            crate::state::AppState::short_did(&did)
        } else {
            nickname.clone()
        };
        app.open_tip(&did, &name);
    }
    if do_message {
        if app.state.chatable_dids.contains(&did) {
            app.start_chat(did.clone());
            let name = if nickname.is_empty() {
                crate::state::AppState::short_did(&did)
            } else {
                nickname.clone()
            };
            app.state.convo.clear();
            let chat = OpenChat { id: did.clone(), name, is_group: false };
            app.load_convo(&chat);
            app.state.open_chat = Some(chat);
            app.state.tab = Tab::Chat;
            app.state.viewed = None;
        } else if let Some(vu) = app.state.viewed.as_mut() {
            // ISOLATION: following someone doesn't open a chat. Guide instead of a dead chat.
            vu.status =
                "Following doesn't open a chat. To message privately, exchange chat QR codes (Chat tab → New chat).".into();
        }
    } else if close {
        app.state.viewed = None;
    }
}

/// 3-column post grid. Each cell: first non-video photo, else a ▶ for video,
/// else a caption snippet — over a dark rounded tile.
fn posts_grid(app: &mut App, ui: &mut egui::Ui, theme: &Theme, posts: &[Value]) {
    let cols = 3;
    let spacing = 4.0;
    let avail = ui.available_width();
    let cell = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(40.0);

    let mut i = 0;
    while i < posts.len() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for _ in 0..cols {
                if i >= posts.len() {
                    break;
                }
                post_cell(app, ui, theme, &posts[i], cell);
                i += 1;
            }
        });
        ui.add_space(spacing);
    }
}

fn post_cell(app: &mut App, ui: &mut egui::Ui, theme: &Theme, post: &Value, cell: f32) {
    let media = post
        .get("media")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let photo_cid = media
        .iter()
        .find(|m| m.get("type").and_then(Value::as_str) != Some("video"))
        .and_then(|m| m.get("cid").and_then(Value::as_str))
        .map(str::to_string);
    let has_video = media
        .iter()
        .any(|m| m.get("type").and_then(Value::as_str) == Some("video"));
    let caption = post.get("caption").and_then(Value::as_str).unwrap_or("");

    if let Some(cid) = photo_cid {
        if let Some(tex) = app.media.texture(&cid, &app.engine, &app.ev_tx) {
            ui.add(
                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                    .fit_to_exact_size(Vec2::splat(cell))
                    .rounding(8.0),
            );
            return;
        }
        // loading placeholder — recessed material tile (matches the profile grid),
        // not a black-alpha scrim that vanishes on the dark canvas.
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cell), egui::Sense::hover());
        ui.painter().rect_filled(rect, 8.0, theme.glass_fill);
        if resp.hovered() {
            ui.painter().rect_filled(rect, 8.0, theme.hover);
        }
        ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, theme.glass_border));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "…",
            egui::FontId::proportional(20.0),
            theme.muted,
        );
        return;
    }

    // No photo: a recessed material tile with a ▶ (video) or a caption snippet.
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cell), egui::Sense::hover());
    ui.painter().rect_filled(rect, 8.0, theme.glass_fill);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8.0, theme.hover);
    }
    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, theme.glass_border));
    if has_video {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icons::PLAY_CIRCLE,
            egui::FontId::proportional(34.0),
            theme.gold_ink,
        );
    } else if !caption.is_empty() {
        let snippet: String = caption.chars().take(18).collect();
        let galley = ui.painter().layout(
            snippet,
            egui::FontId::proportional(10.0),
            theme.muted,
            cell - 8.0,
        );
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, theme.muted);
    }
}
