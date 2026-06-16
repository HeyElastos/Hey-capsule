//! Chat sheets — the AddContactSheet + NewGroupSheet, rendered as centered
//! modal Windows (the desktop analogue of Android's ModalBottomSheet). Surfaced
//! by `app.state.modal == Some(Modal::AddContact | Modal::NewGroup)`.

use egui::{Align2, Color32, FontId, Margin, RichText, Sense};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{Modal, OpenChat};
use crate::theme::{Theme, GOLD, GOLD2, NAVY};

use super::{list_row, primary_button, secondary_button};

// ── Add contact ──────────────────────────────────────────────────────────────

/// "New chat": quick-DM anyone you follow, add by friend-link / invite, and share
/// your own invite link + QR.
pub fn add_contact(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if app.state.modal != Some(Modal::AddContact) {
        return;
    }
    // Fetch the quick-list + invite link once per sheet-open (not every frame).
    let first_open = ctx.data_mut(|d| {
        let seen = d.get_temp::<bool>(egui::Id::new("add-contact-open")).unwrap_or(false);
        d.insert_temp(egui::Id::new("add-contact-open"), true);
        !seen
    });
    if first_open {
        app.load_activity();
        if app.state.friend_link.is_empty() {
            app.load_friend_link();
        }
    }

    let mut close = false;
    egui::Window::new("add-contact")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(520.0);
            theme.sheet_handle(ui);
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("New chat")
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Message someone you follow, paste their Hey friend link, or share your invite.")
                    .size(13.0)
                    .color(theme.muted),
            );

            // Browse / type to start a chat. Bounded so the invite QR below stays
            // fully visible without scrolling (a QR has to be scanned whole).
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // People you follow — already DM-capable (their link carried the keys).
                    let following = app.state.following.clone();
                    if !following.is_empty() {
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new("People you follow")
                                .size(13.0)
                                .family(icons::semibold())
                                .color(theme.muted),
                        );
                        ui.add_space(6.0);
                        for f in &following {
                            let did = f.get("did").and_then(Value::as_str).unwrap_or("").to_string();
                            if did.is_empty() {
                                continue;
                            }
                            if person_row(ui, theme, &did) {
                                app.state.sheets.add_status = "Starting…".into();
                                app.start_chat(did.clone());
                                let name = crate::state::AppState::short_did(&did);
                                app.state.open_chat = Some(OpenChat { id: did, name, is_group: false });
                                app.state.convo.clear();
                                if let Some(c) = app.state.open_chat.clone() {
                                    app.load_convo(&c);
                                    app.load_msg_reactions(&c);
                                }
                                close = true;
                            }
                        }
                        ui.add_space(8.0);
                        ui.separator();
                    }

                    // Add by link or invite.
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Add by link or invite")
                            .size(13.0)
                            .family(icons::semibold())
                            .color(theme.muted),
                    );
                    ui.add_space(10.0);
                    super::field(ui, theme, &mut app.state.sheets.add_input, "Paste a friend link or invite…", 1);
                    let len = app.state.sheets.add_input.trim().len();
                    if len > 24 {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("✓ Link ready ({len} chars)")).size(11.0).color(theme.good));
                    }
                    ui.add_space(12.0);
                    if primary_button(ui, true, "Start chat").clicked() {
                        submit_add(app);
                    }
                    if !app.state.sheets.add_status.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new(app.state.sheets.add_status.clone()).size(13.0).color(theme.muted));
                    }
                });

            // Or share your invite — kept OUTSIDE the scroll area so the whole QR
            // is always shown (mirrors the profile "Show my QR" sheet), capped by
            // viewport height so it never clips on a short window.
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(16.0);
            ui.label(
                RichText::new("Or share your invite")
                    .size(13.0)
                    .family(icons::semibold())
                    .color(theme.muted),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Best: share the link. The QR is dense (it carries your encryption key) — scan close, in good light.")
                    .size(11.0)
                    .color(theme.muted),
            );
            ui.add_space(10.0);

            // QR (rasterised + uploaded on the UI thread, memoised on ctx).
            let link = app.state.friend_link.clone();
            ui.vertical_centered(|ui| {
                if link.is_empty() {
                    ui.add_space(40.0);
                    ui.spinner();
                    ui.add_space(40.0);
                } else if let Some(tex) = crate::qr::qr_texture(ui.ctx(), &link) {
                    // As close to native size as fits: cap by width AND viewport
                    // height (≈480px of sheet chrome — header, follow-list, field,
                    // labels, buttons — sits around it) so the whole sheet fits the
                    // window and the QR is fully visible without scrolling. Floor at
                    // ~220px or the dense code drops under ~3px/module and won't scan.
                    let side = tex
                        .size_vec2()
                        .x
                        .min(ui.available_width())
                        .min((ui.ctx().screen_rect().height() - 480.0).max(220.0));
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .inner_margin(Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                                    .fit_to_exact_size(egui::vec2(side, side)),
                            );
                        });
                } else {
                    ui.label(RichText::new("Use Copy below").color(theme.muted).size(13.0));
                }
            });
            ui.add_space(10.0);
            ui.add_enabled_ui(!link.is_empty(), |ui| {
                if secondary_button(ui, theme, true, "Copy link").clicked() {
                    let l = link.clone();
                    ui.output_mut(|o| o.copied_text = l);
                    app.state.sheets.add_status = "Copied".into();
                }
            });
            ui.add_space(6.0);
        });

    if close {
        app.state.modal = None;
        app.state.sheets.add_input.clear();
        app.state.sheets.add_status.clear();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("add-contact-open"), false));
    }
}

/// Normalize + route a pasted friend-link / invite (mirrors the Android submit()).
fn submit_add(app: &mut App) {
    let v = app.state.sheets.add_input.trim().to_string();
    if v.is_empty() {
        app.state.sheets.add_status = "Paste a friend link or invite".into();
        return;
    }
    if v.starts_with("hey:follow:") {
        // The friend link carries the PQ keys → follow bootstraps a DM.
        app.state.sheets.add_status = "Connecting…".into();
        app.follow(v);
        app.load_chats();
    } else if v.starts_with("hey-invite:") {
        app.state.sheets.add_status = "Connecting…".into();
        app.accept_invite(v);
        app.load_chats();
    } else if v.starts_with("did:") {
        app.state.sheets.add_status = "That's a DID — paste a Hey friend link instead.".into();
    } else {
        app.state.sheets.add_status = "Unrecognized — paste a Hey friend link or scan a Hey QR.".into();
    }
}

/// One "person you follow" row (gradient avatar + short DID). Returns true on tap.
fn person_row(ui: &mut egui::Ui, theme: &Theme, did: &str) -> bool {
    let clicked = list_row(ui, theme, false, |ui| {
        ui.horizontal(|ui| {
            gradient_dot(ui, did, 38.0);
            ui.add_space(10.0);
            ui.label(RichText::new(crate::state::AppState::short_did(did)).size(15.0).color(theme.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(icons::CHAT_BUBBLE_OUTLINE).size(18.0).color(theme.gold_ink));
            });
        });
    })
    .clicked();
    ui.add_space(2.0);
    clicked
}

// ── New group ────────────────────────────────────────────────────────────────

/// Create a group: name + a checkable list of your DM contacts.
pub fn new_group(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if app.state.modal != Some(Modal::NewGroup) {
        return;
    }

    let mut close = false;
    egui::Window::new("new-group")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(470.0);
            theme.sheet_handle(ui);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("New group")
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(12.0);
            super::field(ui, theme, &mut app.state.sheets.group_name, "Group name", 1);
            ui.add_space(14.0);
            ui.label(
                RichText::new("Add members")
                    .size(13.0)
                    .family(icons::semibold())
                    .color(theme.muted),
            );
            ui.add_space(6.0);

            let contacts: Vec<(String, String)> = app
                .state
                .contacts
                .iter()
                .filter_map(|c| {
                    let did = c.get("did").and_then(Value::as_str).unwrap_or("").to_string();
                    if did.is_empty() {
                        return None;
                    }
                    let name = c
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| crate::state::AppState::short_did(&did));
                    Some((did, name))
                })
                .collect();

            if contacts.is_empty() {
                ui.label(
                    RichText::new("Add some contacts first — then you can group them.")
                        .size(13.0)
                        .color(theme.muted),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (did, name) in &contacts {
                            let on = app.state.sheets.group_selected.contains(did);
                            let clicked = list_row(ui, theme, on, |ui| {
                                ui.horizontal(|ui| {
                                    gradient_dot(ui, name, 38.0);
                                    ui.add_space(10.0);
                                    ui.label(RichText::new(name).size(15.0).color(theme.ink));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let (g, c) = if on {
                                                (icons::CHECK_CIRCLE, theme.gold_ink)
                                            } else {
                                                (icons::RADIO_UNCHECKED, theme.muted)
                                            };
                                            ui.label(RichText::new(g).size(20.0).color(c));
                                        },
                                    );
                                });
                            })
                            .clicked();
                            if clicked {
                                if on {
                                    app.state.sheets.group_selected.remove(did);
                                } else {
                                    app.state.sheets.group_selected.insert(did.clone());
                                }
                            }
                            ui.add_space(2.0);
                        }
                    });
            }

            if !app.state.sheets.group_status.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(app.state.sheets.group_status.clone()).size(13.0).color(crate::theme::LIKE));
            }
            ui.add_space(16.0);
            let create = primary_button(ui, true, "Create group").clicked();
            if create {
                let name = app.state.sheets.group_name.trim().to_string();
                let members: Vec<String> = app.state.sheets.group_selected.iter().cloned().collect();
                if name.is_empty() {
                    app.state.sheets.group_status = "Name the group".into();
                } else if members.is_empty() {
                    app.state.sheets.group_status = "Pick at least one member".into();
                } else {
                    app.create_group(name, members);
                    app.load_chats();
                    close = true;
                }
            }
            ui.add_space(6.0);
        });

    if close {
        app.state.modal = None;
        app.state.sheets.group_name.clear();
        app.state.sheets.group_selected.clear();
        app.state.sheets.group_status.clear();
    }
}

// ── shared widgets ───────────────────────────────────────────────────────────

/// Small clean gradient avatar dot (no gloss) with a leading character (DID or name).
fn gradient_dot(ui: &mut egui::Ui, label: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let c = rect.center();
    super::gradient_circle(ui.painter(), c, size / 2.0, GOLD2, GOLD);
    let ch = label
        .strip_prefix("did:key:z")
        .unwrap_or(label)
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    ui.painter()
        .text(c, Align2::CENTER_CENTER, ch, FontId::new(size * 0.42, icons::semibold()), NAVY);
}

