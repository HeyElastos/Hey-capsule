//! Profile tab ("You") — a desktop-professional profile: a hero identity card
//! (avatar + name + status + actions), a stat row (Posts / Followers / Following /
//! Chats), then a two-column body — your post grid on the left, your account
//! (Security / Connection / About / Appearance) + people on the right.

use egui::{Align, Color32, Layout, RichText, Sense, Stroke};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{AppState, Modal, Tab, ViewedUser};
use crate::theme::Theme;

use super::avatar;

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // One-shot in-flight guard: `activity_loaded` only flips many frames after the
    // two engine calls land, so without this we'd re-dispatch them every frame the
    // tab is shown and flood the 3-worker pool. A transient egui-memory flag (mirrors
    // wallet.rs's "wallet-load-started") makes the fetch fire at most once until it
    // resolves; the event handler clears it by setting `activity_loaded = true`.
    if !app.state.activity_loaded {
        let load_id = egui::Id::new("profile-activity-started");
        let started = ui.ctx().memory(|m| m.data.get_temp::<bool>(load_id).unwrap_or(false));
        if !started {
            ui.ctx().memory_mut(|m| m.data.insert_temp(load_id, true));
            app.load_activity();
        }
    }

    let out = egui::ScrollArea::vertical()
        .id_source("profile-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Cap the profile to a comfortable reading width, centered.
            let avail = ui.available_width();
            let content_w = avail.min(1080.0);
            let pad = ((avail - content_w) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(content_w);

                    hero(app, ui, theme);
                    ui.add_space(16.0);
                    stat_row(app, ui, theme);
                    ui.add_space(20.0);

                    // Two-column body: posts (left, wider) | account + people (right).
                    let gap = 24.0_f32;
                    let left_w = ((content_w - gap) * 0.58).max(300.0);
                    let right_w = (content_w - gap - left_w).max(280.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2(left_w, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(left_w);
                            posts_grid(app, ui, theme);
                        });
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(egui::vec2(right_w, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(right_w);
                            account_section(app, ui, theme);
                        });
                    });
                    ui.add_space(30.0);
                });
            });
        });

    // Feed the collapsing large-title (content_header consumes "view-scroll-y").
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("view-scroll-y"), out.state.offset.y));
}

// ── hero identity card ────────────────────────────────────────────────────────
fn hero(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let prof = app.state.profile.clone();
    let nickname = prof.get("nickname").and_then(Value::as_str).unwrap_or("");
    let bio = prof.get("bio").and_then(Value::as_str).unwrap_or("");
    let av = prof.get("avatar").and_then(Value::as_str).unwrap_or("").to_string();
    let me_did = app.state.me_did.clone();
    let online = app.state.online;
    let peers = app.state.peers;

    let mut open_settings = false;
    let mut open_edit = false;
    let mut open_qr = false;
    let mut open_add = false;

    theme.material_raised(20.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.scope(|ui| {
                // Self avatar — the gold self-ring is drawn automatically (me-did).
                avatar(&mut app.media, &app.engine, &app.ev_tx, ui, &av, &me_did, 92.0);
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if nickname.is_empty() { "You" } else { nickname })
                            .size(22.0)
                            .family(icons::display())
                            .color(theme.ink),
                    );
                    ui.add_space(10.0);
                    // Status → recessed pill chip.
                    let (txt, col) = if online {
                        (format!("● {peers} online"), theme.good)
                    } else {
                        ("○ connecting…".to_string(), theme.gold_ink)
                    };
                    super::chip(ui, theme, &txt, col);
                });
                ui.add_space(3.0);
                // DID monospace muted (not gold).
                ui.label(RichText::new(AppState::short_did(&me_did)).size(12.0).color(theme.muted).monospace());
                if !bio.is_empty() {
                    ui.add_space(9.0);
                    ui.add(egui::Label::new(RichText::new(bio).size(14.0).color(theme.ink)).wrap());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if super::icon_button(ui, theme, icons::SETTINGS, 19.0, theme.muted)
                    .on_hover_text("Settings")
                    .clicked()
                {
                    open_settings = true;
                }
            });
        });

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if super::primary_button(ui, false, &format!("{}  Add friend", icons::PERSON_ADD)).clicked() {
                open_add = true;
            }
            ui.add_space(8.0);
            if super::secondary_button(ui, theme, false, &format!("{}  Edit profile", icons::EDIT)).clicked() {
                open_edit = true;
            }
            ui.add_space(8.0);
            if super::outline_button(ui, theme, false, &format!("{}  Invite QR", icons::QR_CODE_2)).clicked() {
                open_qr = true;
            }
        });
    });

    if open_settings {
        app.state.modal = Some(Modal::Settings);
    }
    if open_edit {
        prefill_draft(app);
        app.state.modal = Some(Modal::EditProfile);
    }
    if open_qr {
        app.state.modal = Some(Modal::MyQr);
    }
    if open_add {
        app.state.sheets.add_input.clear();
        app.state.sheets.add_status.clear();
        app.state.modal = Some(Modal::AddFriend);
    }
}

// ── stat row ──────────────────────────────────────────────────────────────────
fn stat_row(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let me = app.state.me_did.clone();
    let posts_n = app
        .state
        .feed
        .iter()
        .filter(|p| !me.is_empty() && p.get("author").and_then(Value::as_str) == Some(me.as_str()))
        .count();
    let followers_n = app.state.followers.len();
    let following_n = app.state.following.len();
    let chats_n = app.state.contacts.len();

    // One grouped material card with 4 equal segments + inset hairline dividers.
    let cells = [
        (posts_n, "Posts"),
        (followers_n, "Followers"),
        (following_n, "Following"),
        (chats_n, "Chats"),
    ];
    theme.glass(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let row = ui.available_rect_before_wrap();
        let seg_w = row.width() / cells.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (n, label) in cells {
                ui.allocate_ui_with_layout(
                    egui::vec2(seg_w, 0.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.set_width(seg_w);
                        ui.label(
                            RichText::new(n.to_string())
                                .size(23.0)
                                .family(icons::semibold())
                                .color(theme.ink),
                        );
                        ui.add_space(1.0);
                        ui.label(RichText::new(label).size(12.0).color(theme.muted));
                    },
                );
            }
        });
        // Inset vertical hairlines between the 3 inner boundaries.
        let r = ui.min_rect();
        let inset = 12.0;
        let p = ui.painter();
        for i in 1..cells.len() {
            let x = r.left() + seg_w * i as f32;
            p.vline(
                x,
                (r.top() + inset)..=(r.bottom() - inset),
                Stroke::new(1.0, theme.glass_border),
            );
        }
    });
}

// ── right column: account + people ────────────────────────────────────────────
fn account_section(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let online = app.state.online;

    // Security detail card (informational — its sub-rows have no destination).
    theme.glass(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::VERIFIED_USER).size(18.0).color(theme.good));
            ui.add_space(6.0);
            ui.label(RichText::new("Security").size(15.0).family(icons::semibold()).color(theme.ink));
        });
        ui.add_space(8.0);
        sec_row(ui, theme, "Encryption", "End-to-end · ML-KEM-768 + X25519");
        sec_row(ui, theme, "Keys", "Held on this device, never uploaded");
        sec_row(ui, theme, "Identity", "Self-sovereign did:key — owned by you");
    });

    ui.add_space(12.0);

    // Account group — Connection + About as one material group of list_rows.
    let mut open_conn = false;
    let mut open_about = false;
    theme.glass(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());

        // Connection (tap → Connection sheet).
        let conn = super::list_row(ui, theme, false, |ui| {
            ui.label(RichText::new(icons::HUB).size(18.0).color(theme.gold_ink));
            ui.add_space(10.0);
            ui.label(RichText::new("Connection").size(15.0).family(icons::semibold()).color(theme.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(icons::CHEVRON_RIGHT).size(18.0).color(theme.faint));
                ui.add_space(4.0);
                if online {
                    ui.label(RichText::new("● Live").size(12.0).color(theme.good));
                } else {
                    ui.label(RichText::new("○ Connecting").size(12.0).color(theme.muted));
                }
            });
        });
        if conn.clicked() {
            open_conn = true;
        }

        // Inset hairline between rows.
        let sep = ui.cursor().top();
        ui.painter().hline(
            (ui.min_rect().left() + 12.0)..=(ui.min_rect().right() - 12.0),
            sep,
            Stroke::new(1.0, theme.glass_border),
        );

        // About (tap → About sheet).
        let about = super::list_row(ui, theme, false, |ui| {
            ui.label(RichText::new(icons::INFO).size(18.0).color(theme.gold_ink));
            ui.add_space(10.0);
            ui.label(RichText::new("About Hey").size(15.0).family(icons::semibold()).color(theme.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(icons::CHEVRON_RIGHT).size(18.0).color(theme.faint));
            });
        });
        if about.clicked() {
            open_about = true;
        }
    });
    if open_conn {
        app.state.modal = Some(Modal::Connection);
    }
    if open_about {
        app.state.modal = Some(Modal::About);
    }

    ui.add_space(12.0);

    // Appearance — segmented Light/Dark (matches welcome).
    theme.glass(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Appearance").size(13.0).color(theme.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let sel = if app.state.light { 0 } else { 1 };
                if let Some(i) = super::segmented(ui, theme, "appearance", &["Light", "Dark"], sel) {
                    app.state.light = i == 0;
                    crate::theme::save_pref(app.state.light); // persist across launches
                }
            });
        });
    });

    ui.add_space(20.0);

    // Followers.
    let followers = app.state.followers.clone();
    if !followers.is_empty() {
        section_title(ui, theme, &format!("Followers ({})", followers.len()));
        for f in &followers {
            let did = f.get("did").and_then(Value::as_str).unwrap_or("").to_string();
            if did.is_empty() {
                continue;
            }
            if person_row(app, ui, theme, &did, false) {
                open_user(app, &did);
            }
        }
        ui.add_space(20.0);
    }

    // Following.
    let following = app.state.following.clone();
    section_title(ui, theme, &format!("Following ({})", following.len()));
    if following.is_empty() {
        ui.label(RichText::new("Not following anyone yet.").size(12.0).color(theme.muted));
    }
    for f in &following {
        let did = f.get("did").and_then(Value::as_str).unwrap_or("").to_string();
        if did.is_empty() {
            continue;
        }
        let (tapped, unfollowed) = person_row_unfollow(app, ui, theme, &did);
        if unfollowed {
            app.unfollow(did.clone());
            app.load_activity();
        } else if tapped {
            open_user(app, &did);
        }
    }
}

fn section_title(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(RichText::new(text).size(13.0).family(icons::semibold()).color(theme.muted));
    ui.add_space(8.0);
}

// ── post grid (the user's own photos) ─────────────────────────────────────────
fn posts_grid(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let me = app.state.me_did.clone();
    let mine: Vec<Value> = app
        .state
        .feed
        .iter()
        .filter(|p| !me.is_empty() && p.get("author").and_then(Value::as_str) == Some(me.as_str()))
        .cloned()
        .collect();

    section_title(ui, theme, &format!("Your posts ({})", mine.len()));

    if mine.is_empty() {
        theme.glass(16.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            super::empty_state(
                ui,
                theme,
                icons::PHOTO_CAMERA,
                "No posts yet",
                "Share a photo from Feed → New post.",
            );
            ui.add_space(20.0);
        });
        return;
    }

    let avail = ui.available_width();
    let cols = if avail > 560.0 { 4usize } else { 3usize };
    let spacing = 6.0_f32;
    let cell = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).floor().max(60.0);
    let mut open_post: Option<String> = None;

    for chunk in mine.chunks(cols) {
        ui.horizontal(|ui| {
            for post in chunk {
                let pid = post.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                let cid = post.get("media").and_then(Value::as_array).and_then(|m| {
                    m.iter().find_map(|t| {
                        let c = t.get("cid").and_then(Value::as_str).unwrap_or("");
                        let vid = t.get("type").and_then(Value::as_str) == Some("video");
                        (!c.is_empty() && !vid).then(|| c.to_string())
                    })
                });
                if grid_cell(app, ui, theme, cell, cid.as_deref()) && !pid.is_empty() {
                    open_post = Some(pid);
                }
                ui.add_space(spacing);
            }
        });
        ui.add_space(spacing);
    }

    if let Some(id) = open_post {
        app.state.tab = Tab::Feed;
        app.state.feed_scroll_to = Some(id);
    }
}

/// One square post thumbnail (cover-fit, cropped, clipped). Returns true on click.
fn grid_cell(app: &mut App, ui: &mut egui::Ui, theme: &Theme, size: f32, cid: Option<&str>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 12.0, theme.glass_fill);
    if let Some(c) = cid {
        if let Some(tex) = app.media.texture(c, &app.engine, &app.ev_tx) {
            let s = tex.size_vec2();
            let f = (size / s.x.max(1.0)).max(size / s.y.max(1.0));
            let d = s * f;
            let ir = egui::Rect::from_center_size(rect.center(), d);
            painter.image(
                tex.id(),
                ir,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(20.0), theme.muted);
        }
    } else {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, icons::DESCRIPTION, egui::FontId::proportional(22.0), theme.muted);
    }
    if resp.hovered() {
        painter.rect_filled(rect, 12.0, Color32::from_black_alpha(20));
    }
    painter.rect_stroke(rect, 12.0, Stroke::new(1.0, theme.glass_border));
    resp.clicked()
}

// ── helpers ──────────────────────────────────────────────────────────────────
fn prefill_draft(app: &mut App) {
    let prof = app.state.profile.clone();
    let d = &mut app.state.profile_draft;
    d.nickname = prof.get("nickname").and_then(Value::as_str).unwrap_or("").to_string();
    d.bio = prof.get("bio").and_then(Value::as_str).unwrap_or("").to_string();
    d.avatar_cid = prof.get("avatar").and_then(Value::as_str).unwrap_or("").to_string();
    d.avatar_bytes = None;
    d.busy = false;
    d.loaded = true;
}

fn open_user(app: &mut App, did: &str) {
    app.state.viewed = Some(ViewedUser {
        did: did.to_string(),
        ..Default::default()
    });
    app.load_user(did);
}

fn sec_row(ui: &mut egui::Ui, theme: &Theme, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 18.0],
            egui::Label::new(RichText::new(k).size(13.0).color(theme.muted)).halign(egui::Align::LEFT),
        );
        ui.add(egui::Label::new(RichText::new(v).size(13.0).color(theme.ink)).wrap());
    });
    ui.add_space(3.0);
}

fn person_row(app: &mut App, ui: &mut egui::Ui, theme: &Theme, did: &str, _trailing: bool) -> bool {
    let resp = super::list_row(ui, theme, false, |ui| {
        avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", did, 34.0);
        ui.add_space(10.0);
        ui.label(RichText::new(person_label(did)).size(13.0).color(theme.ink));
    });
    // Avatar/row context menu — View profile (= the row click) + Copy DID. Both are
    // read-only here; the click path already opens the profile.
    let mut open_profile = false;
    let mut copy_did = false;
    let d = did.to_string();
    resp.context_menu(|ui| {
        ui.set_min_width(160.0);
        if super::menu_item(ui, theme, icons::PERSON, "View profile", "", false).clicked() {
            open_profile = true;
            ui.close_menu();
        }
        if super::menu_item(ui, theme, icons::CONTENT_COPY, "Copy DID", "", false).clicked() {
            copy_did = true;
            ui.close_menu();
        }
    });
    if copy_did {
        ui.ctx().output_mut(|o| o.copied_text = d);
    }
    open_profile || resp.clicked()
}

fn person_row_unfollow(app: &mut App, ui: &mut egui::Ui, theme: &Theme, did: &str) -> (bool, bool) {
    let mut unfollowed = false;
    let resp = super::list_row(ui, theme, false, |ui| {
        avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", did, 34.0);
        ui.add_space(10.0);
        ui.label(RichText::new(person_label(did)).size(13.0).color(theme.ink));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Unfollow").size(12.0).color(theme.muted)).frame(false))
                .clicked()
            {
                unfollowed = true;
            }
        });
    });
    let tapped = !unfollowed && resp.clicked();
    (tapped, unfollowed)
}

fn person_label(did: &str) -> String {
    let body = did.strip_prefix("did:key:z").unwrap_or(did);
    let head: String = body.chars().take(18).collect();
    format!("{head}…")
}
