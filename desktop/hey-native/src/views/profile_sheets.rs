//! Profile-tab sheets, rendered as centered egui Windows (the Android
//! ModalBottomSheets): Edit profile, My QR, Add friend, Settings, Connection,
//! and About. Each is gated by the matching `Modal` variant in `app.state.modal`
//! by the integrator; closing sets `app.state.modal = None`.

use egui::{Align2, Color32, Margin, Pos2, RichText, Stroke, Vec2};

use crate::app::App;
use crate::icons;
use crate::state::Modal;
use crate::theme::{Theme, GOLD, GOLD2, NAVY};

// ── shared sheet chrome ───────────────────────────────────────────────────────

/// Header row: the iPad grabber handle, then a 20px SemiBold title on the left and
/// a "✕" close button on the right. Returns true when close is pressed.
fn header(ui: &mut egui::Ui, theme: &Theme, title: &str) -> bool {
    theme.sheet_handle(ui);
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(20.0).family(icons::semibold()).color(theme.ink));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if super::icon_button(ui, theme, icons::CLOSE, 16.0, theme.muted).clicked() {
                close = true;
            }
        });
    });
    close
}

/// The slide-up entrance offset for an iPad form-sheet (CENTER_CENTER anchor).
fn rise(ctx: &egui::Context) -> Vec2 {
    Vec2::new(0.0, crate::app::sheet_rise(ctx))
}

/// A glass sub-card with an icon + title header (used inside the sheets).
fn card_header(ui: &mut egui::Ui, theme: &Theme, glyph: &str, glyph_col: Color32, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(glyph).size(18.0).color(glyph_col));
        ui.add_space(6.0);
        ui.label(RichText::new(title).size(15.0).family(icons::semibold()).color(theme.ink));
    });
}

/// Android `AboutItem` / `ConnStep` leading icon: a gold gradient circle with a
/// dark glyph. egui can't gradient-fill a button, so we paint two stacked
/// circles (Gold over Gold2) and draw the glyph centred.
fn gradient_icon(ui: &mut egui::Ui, glyph: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, GOLD2);
    p.circle_filled(rect.center() + Vec2::new(0.0, -size * 0.12), size / 2.0 * 0.92, GOLD);
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(size * 0.52),
        NAVY,
    );
}

/// Title + body paragraph block used by ConnStep and AboutItem.
fn icon_step(ui: &mut egui::Ui, theme: &Theme, glyph: &str, title: &str, body: &str) {
    ui.horizontal_top(|ui| {
        gradient_icon(ui, glyph, 30.0);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(15.0).family(icons::semibold()).color(theme.ink));
            ui.label(RichText::new(body).size(13.0).color(theme.muted));
        });
    });
    ui.add_space(6.0);
}

// ── Edit profile ──────────────────────────────────────────────────────────────

pub fn edit_profile(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    egui::Window::new("edit_profile")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            if header(ui, theme, "Edit profile") {
                open = false;
            }
            ui.add_space(16.0);

            // Tappable avatar (88) — pick a new image.
            ui.vertical_centered(|ui| {
                let cid = app.state.profile_draft.avatar_cid.clone();
                let did = app.state.me_did.clone();
                let tex = if !cid.is_empty() {
                    app.media.texture(&cid, &app.engine, &app.ev_tx)
                } else {
                    None
                };
                let resp = if let Some(t) = tex {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::from_handle(&t))
                            .fit_to_exact_size(Vec2::splat(88.0))
                            .rounding(44.0)
                            .sense(egui::Sense::click()),
                    )
                } else {
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::splat(88.0), egui::Sense::click());
                    let p = ui.painter();
                    p.circle_filled(rect.center(), 44.0, GOLD2);
                    p.circle_filled(rect.center() + Vec2::new(0.0, -10.0), 40.0, GOLD);
                    let glyph = if did.is_empty() { icons::ADD } else { icons::PHOTO_CAMERA };
                    p.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        glyph,
                        egui::FontId::proportional(28.0),
                        NAVY,
                    );
                    resp
                };
                if resp.clicked() {
                    app.pick_avatar();
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Click to change photo").size(11.0).color(theme.muted));
            });

            ui.add_space(18.0);
            ui.label(RichText::new("Name").size(13.0).family(icons::semibold()).color(theme.muted));
            ui.add_space(6.0);
            super::field(ui, theme, &mut app.state.profile_draft.nickname, "Your name", 1);
            ui.add_space(14.0);
            ui.label(RichText::new("Bio").size(13.0).family(icons::semibold()).color(theme.muted));
            ui.add_space(6.0);
            super::field(ui, theme, &mut app.state.profile_draft.bio, "A short bio", 3);
            ui.add_space(18.0);

            let busy = app.state.profile_draft.busy;
            let save = super::primary_button(ui, true, if busy { "Saving…" } else { "Save" });
            if save.clicked() && !busy {
                let nick = {
                    let n = app.state.profile_draft.nickname.trim();
                    if n.is_empty() { "Hey user".to_string() } else { n.to_string() }
                };
                let bio = app.state.profile_draft.bio.trim().to_string();
                let avatar = app.state.profile_draft.avatar_cid.clone();
                app.set_profile(nick, bio, avatar);
                open = false;
            }
        });
    if !open {
        app.state.modal = None;
    }
}

// ── My QR ──────────────────────────────────────────────────────────────────────

pub fn my_qr(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    let link = app.state.friend_link.clone();
    egui::Window::new("my_qr")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            // Wide enough to host the friend-link QR at native size (~520px) —
            // shrinking the dense code below ~3px/module makes it unscannable.
            ui.set_max_width(560.0);
            if header(ui, theme, "Add me on Hey") {
                open = false;
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Send the link, or scan the QR up close in good light.")
                    .size(12.0)
                    .color(theme.muted),
            );
            ui.add_space(14.0);

            // White QR box (as wide as the sheet → most scannable).
            let box_w = ui.available_width();
            egui::Frame::none()
                .fill(Color32::WHITE)
                .rounding(16.0)
                .inner_margin(Margin::same(12.0))
                .show(ui, |ui| {
                    ui.set_width(box_w - 24.0);
                    ui.vertical_centered(|ui| {
                        if link.is_empty() {
                            ui.add_space(40.0);
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(RichText::new("preparing…").size(12.0).color(NAVY));
                            ui.add_space(40.0);
                        } else if let Some(tex) = crate::qr::qr_texture(ctx, &link) {
                            // As close to native texture size as the window
                            // allows: ~220pt of sheet chrome (header/label/
                            // button/margins) sits around the QR, so cap the
                            // side by the viewport height or the popup clips.
                            let side = tex
                                .size_vec2()
                                .x
                                .min(box_w - 36.0)
                                .min(ctx.screen_rect().height() - 220.0);
                            ui.add(
                                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                                    .fit_to_exact_size(Vec2::splat(side)),
                            );
                        } else {
                            ui.add_space(40.0);
                            ui.label(RichText::new("Use Copy below").size(13.0).color(NAVY));
                            ui.add_space(40.0);
                        }
                    });
                });

            ui.add_space(16.0);
            ui.add_enabled_ui(!link.is_empty(), |ui| {
                if super::primary_button(ui, true, &format!("{}  Copy invite link", icons::CONTENT_COPY)).clicked() {
                    let l = link.clone();
                    ui.output_mut(|o| o.copied_text = l);
                }
            });
            ui.add_space(2.0);
        });
    if !open {
        app.state.modal = None;
    }
}

// ── Add friend ──────────────────────────────────────────────────────────────────

pub fn add_friend(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    egui::Window::new("add_friend")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            if header(ui, theme, "Follow someone") {
                open = false;
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Paste their Hey friend link.")
                    .size(13.0)
                    .color(theme.muted),
            );
            ui.add_space(14.0);
            super::field(ui, theme, &mut app.state.sheets.add_input, "hey:follow:…", 1);

            let len = app.state.sheets.add_input.trim().len();
            if len > 24 {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{} Link ready ({len} chars)", icons::CHECK))
                        .size(11.0)
                        .color(theme.good),
                );
            }
            ui.add_space(12.0);

            let mut do_follow = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::primary_button(ui, false, "Follow").clicked() {
                    do_follow = true;
                }
            });

            if do_follow {
                let v = app.state.sheets.add_input.trim().to_string();
                if v.is_empty() {
                    app.state.sheets.add_status = "Paste a Hey friend link".into();
                } else if v.starts_with("did:") && !v.contains("hey:follow") {
                    app.state.sheets.add_status =
                        "That's a DID — it can't start a private channel. Ask them for their Hey friend link or QR.".into();
                } else {
                    app.follow(v);
                    app.load_activity();
                    open = false;
                }
            }

            if !app.state.sheets.add_status.is_empty() {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(app.state.sheets.add_status.clone())
                        .size(13.0)
                        .color(theme.muted),
                );
            }
        });
    if !open {
        app.state.modal = None;
    }
}

// ── Settings ──────────────────────────────────────────────────────────────────

pub fn settings(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    let mut goto_qr = false;
    let mut goto_conn = false;
    let did = app.state.me_did.clone();
    egui::Window::new("settings")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(470.0);
            if header(ui, theme, "Settings") {
                open = false;
            }
            ui.add_space(16.0);

            // Your identity card.
            theme.glass(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                card_header(ui, theme, icons::BADGE, theme.gold_ink, "Your identity");
                ui.add_space(8.0);
                ui.label(RichText::new(&did).size(12.0).color(theme.muted));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if super::outline_button(ui, theme, false, &format!("{} Copy DID", icons::CONTENT_COPY)).clicked() {
                        let d = did.clone();
                        ui.output_mut(|o| o.copied_text = d);
                    }
                    ui.add_space(8.0);
                    if super::secondary_button(ui, theme, false, &format!("{} My QR", icons::QR_CODE_2)).clicked() {
                        goto_qr = true;
                    }
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new("This DID is your sovereign identity — it signs everything you create. To connect, share your invite link or QR; a DID alone can't open a private channel.")
                        .size(12.0)
                        .color(theme.muted),
                );
            });

            ui.add_space(12.0);

            // How Hey connects (tap → Connection) — grouped iPad list row.
            theme.glass(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let conn = super::list_row(ui, theme, false, |ui| {
                    ui.label(RichText::new(icons::HUB).size(18.0).color(theme.gold_ink));
                    ui.add_space(10.0);
                    ui.label(RichText::new("How Hey connects").size(15.0).family(icons::semibold()).color(theme.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(icons::CHEVRON_RIGHT).size(18.0).color(theme.faint));
                    });
                });
                if conn.clicked() {
                    goto_conn = true;
                }
            });

            ui.add_space(12.0);

            // Appearance — segmented Light/Dark.
            ui.horizontal(|ui| {
                ui.label(RichText::new("Appearance").size(13.0).color(theme.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sel = if app.state.light { 0 } else { 1 };
                    if let Some(i) = super::segmented(ui, theme, "settings-appearance", &["Light", "Dark"], sel) {
                        app.state.light = i == 0;
                        crate::theme::save_pref(app.state.light); // persist across launches
                    }
                });
            });
        });

    if goto_qr {
        app.state.modal = Some(Modal::MyQr);
    } else if goto_conn {
        app.state.modal = Some(Modal::Connection);
    } else if !open {
        app.state.modal = None;
    }
}

// ── Connection ──────────────────────────────────────────────────────────────────

pub fn connection(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    let online = app.state.online;
    let direct = app.state.direct;
    let peers = app.state.peers;
    egui::Window::new("connection")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(470.0);
            if header(ui, theme, "How Hey connects") {
                open = false;
            }
            ui.add_space(2.0);
            ui.label(
                RichText::new("No servers store your data. Your device is the node.")
                    .size(13.0)
                    .color(theme.muted),
            );
            ui.add_space(16.0);

            // Diagram: relay introduces (dashed), devices talk directly (solid).
            connection_diagram(ui, theme, direct);

            ui.add_space(14.0);
            icon_step(
                ui,
                theme,
                icons::HUB,
                "Relay introduces",
                "The relay finds your friend's device and helps the two punch through firewalls/NAT. It's a matchmaker — it never stores your account or messages.",
            );
            icon_step(
                ui,
                theme,
                icons::SWAP_HORIZ,
                "Carrier connects",
                "Your two devices form a direct peer-to-peer link — the Carrier (iroh). Once joined, messages and media flow device-to-device.",
            );
            icon_step(
                ui,
                theme,
                icons::LOCK,
                "End-to-end encrypted",
                "Everything is sealed with ML-KEM-768 + X25519. Even when traffic must pass a relay, it only ever sees ciphertext — never your content.",
            );

            ui.add_space(14.0);
            theme.glass(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let dot = if online { theme.good } else { theme.muted };
                    let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, dot);
                    ui.add_space(6.0);
                    if online {
                        ui.label(
                            RichText::new(format!("Live on the carrier · {peers} connected"))
                                .size(13.0)
                                .family(icons::semibold())
                                .color(theme.good),
                        );
                    } else {
                        ui.label(
                            RichText::new("Connecting to the carrier…")
                                .size(13.0)
                                .family(icons::semibold())
                                .color(theme.gold_ink),
                        );
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(if direct {
                        "Direct mode: data is travelling peer-to-peer. The relay is only introducing devices."
                    } else {
                        "Relay-assisted: this network blocks direct connections, so encrypted data currently rides the relay. It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link."
                    })
                    .size(12.0)
                    .color(theme.muted),
                );
            });

            // ── live network diagnostics (real IP, direct/relay, bound interface) ──
            //    Mirrors the Android connection sheet so you can verify, on desktop
            //    too, which path is carrying traffic and which interface Hey binds.
            ui.add_space(12.0);
            theme.glass(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let s = &app.state;
                let netlabel = if s.ipv6_global && s.ipv4 {
                    "IPv6 + IPv4"
                } else if s.ipv6_global {
                    "IPv6 (global)"
                } else if s.ipv4 {
                    "IPv4 (behind NAT)"
                } else {
                    "—"
                };
                ui.label(
                    RichText::new(format!("Network: {netlabel}"))
                        .size(12.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                if !s.public_v6.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Public IPv6   {}", s.public_v6)).size(11.0).monospace().color(theme.good));
                }
                if !s.public_v4.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Public IPv4   {}", s.public_v4)).size(11.0).monospace().color(theme.gold_ink));
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Live links: {} direct · {} relayed", s.direct_peers, s.relay_peers))
                        .size(12.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.add_space(3.0);
                let udp = {
                    let mut v: Vec<&str> = Vec::new();
                    if s.udp_v4 { v.push("IPv4"); }
                    if s.udp_v6 { v.push("IPv6"); }
                    if v.is_empty() { "none yet — relay only".to_string() } else { v.join(" + ") }
                };
                ui.label(RichText::new(format!("Direct UDP path: {udp}")).size(11.0).color(theme.muted));
                if !s.local_addrs.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Address Hey is using").size(12.0).family(icons::semibold()).color(theme.ink));
                    for a in &s.local_addrs {
                        ui.label(RichText::new(format!("• {a}")).size(11.0).monospace().color(theme.muted));
                    }
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("The interface Hey binds: on a VPN you'll see the tunnel address (10.x); on plain Wi-Fi your LAN address. The Public IP above is your egress.")
                            .size(10.0)
                            .color(theme.muted),
                    );
                }
            });

            // ── Relay (custom relay URL; iroh "standard" n0 relays off until 1.0) ──
            ui.add_space(18.0);
            ui.label(RichText::new("Relay").size(13.0).family(icons::semibold()).color(theme.ink));
            ui.add_space(2.0);
            ui.label(
                RichText::new("Hey pins the version-matched elastos.app relay. Set your own below.")
                    .size(11.0)
                    .color(theme.muted),
            );
            ui.add_space(8.0);
            let rid = egui::Id::new("relay-draft");
            let mut relay = match ui.ctx().data(|d| d.get_temp::<String>(rid)) {
                Some(s) => s,
                None => dirs::data_dir()
                    .and_then(|d| {
                        std::fs::read_to_string(
                            d.join("hey-social-native").join("carrier").join("relay-url.txt"),
                        )
                        .ok()
                    })
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "https://elastos.app".to_string()),
            };
            super::field(ui, theme, &mut relay, "https://elastos.app", 1);
            ui.ctx().data_mut(|d| d.insert_temp(rid, relay.clone()));
            ui.add_space(10.0);

            // iroh "standard" (n0) relays — disabled until iroh ships a tagged 1.0.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  iroh standard relays (n0)", icons::PUBLIC))
                        .size(13.0)
                        .color(theme.faint),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    super::chip(ui, theme, "Off · until iroh 1.0", theme.faint);
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("n0's public relays ride iroh's pre-release schedule; a version skew there breaks the connection, so they're pinned off.")
                    .size(11.0)
                    .color(theme.faint),
            );
            ui.add_space(12.0);
            if super::primary_button(ui, true, "Save relay").clicked() {
                if let Some(dir) = dirs::data_dir().map(|d| d.join("hey-social-native").join("carrier")) {
                    std::fs::create_dir_all(&dir).ok();
                    let v = relay.trim();
                    if v.is_empty() {
                        std::fs::remove_file(dir.join("relay-url.txt")).ok();
                    } else {
                        std::fs::write(dir.join("relay-url.txt"), v).ok();
                    }
                }
                let now = ctx.input(|i| i.time);
                app.state.toast = Some(("Relay saved — restart Hey to apply".into(), now + 3.0));
            }
        });
    if !open {
        app.state.modal = None;
    }
}

/// You — Relay — Friend triangle: dashed legs to the relay, a solid You↔Friend
/// base that turns gold when the link is direct.
fn connection_diagram(ui: &mut egui::Ui, theme: &Theme, direct: bool) {
    let w = ui.available_width();
    let h = 150.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::hover());
    // glass backing
    ui.painter().rect_filled(rect, 14.0, theme.glass_fill);
    ui.painter()
        .rect_stroke(rect, 14.0, Stroke::new(1.0, theme.glass_border));

    let you = Pos2::new(rect.left() + w * 0.15, rect.top() + h * 0.78);
    let friend = Pos2::new(rect.left() + w * 0.85, rect.top() + h * 0.78);
    let relay = Pos2::new(rect.left() + w * 0.50, rect.top() + h * 0.20);
    let p = ui.painter();

    // dashed introduce-lines (approximated with short segments)
    dashed_line(p, you, relay, theme.muted.gamma_multiply(0.6));
    dashed_line(p, friend, relay, theme.muted.gamma_multiply(0.6));
    // solid base
    let base_col = if direct { theme.gold_ink } else { theme.muted.gamma_multiply(0.5) };
    p.line_segment([you, friend], Stroke::new(6.0, base_col));

    // node chips
    chip(p, theme, relay, &format!("{} Relay", icons::HUB));
    chip(p, theme, you, &format!("{} You", icons::SMARTPHONE));
    chip(p, theme, friend, &format!("{} Friend", icons::SMARTPHONE));

    // mode label centred on the base
    let mid = Pos2::new((you.x + friend.x) / 2.0, you.y + 12.0);
    let label = if direct { "direct · encrypted" } else { "relayed · encrypted" };
    let col = if direct { theme.good } else { theme.muted };
    p.text(mid, Align2::CENTER_CENTER, label, egui::FontId::proportional(10.0), col);
}

fn dashed_line(p: &egui::Painter, a: Pos2, b: Pos2, col: Color32) {
    let dir = b - a;
    let len = dir.length();
    if len < 1.0 {
        return;
    }
    let step = 12.0;
    let n = (len / step).floor() as i32;
    let unit = dir / len;
    for i in 0..n {
        if i % 2 == 0 {
            let s = a + unit * (i as f32 * step);
            let e = a + unit * (((i + 1) as f32 * step).min(len));
            p.line_segment([s, e], Stroke::new(3.0, col));
        }
    }
}

fn chip(p: &egui::Painter, theme: &Theme, center: Pos2, text: &str) {
    let galley = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(12.0), theme.ink);
    let pad = Vec2::new(8.0, 5.0);
    let size = galley.size() + pad * 2.0;
    let rect = egui::Rect::from_center_size(center, size);
    p.rect_filled(rect, 12.0, theme.glass_fill);
    p.rect_stroke(rect, 12.0, Stroke::new(1.0, theme.glass_border));
    p.galley(rect.min + pad, galley, theme.ink);
}

// ── About ──────────────────────────────────────────────────────────────────────

pub fn about(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let mut open = true;
    let online = app.state.online;
    let direct = app.state.direct;
    egui::Window::new("about")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, rise(ctx))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(480.0);
            if header(ui, theme, "About Hey") {
                open = false;
            }
            ui.add_space(2.0);
            ui.label(
                RichText::new("Built on the Elastos Internet OS — you own your identity, your data, and your connections.")
                    .size(13.0)
                    .color(theme.muted),
            );
            ui.add_space(16.0);

            about_item(
                ui,
                theme,
                icons::COMPUTER,
                "It runs on your machine",
                "There is no Hey server. A mini Elastos runtime + the Carrier (the peer-to-peer network) run inside the app, on your device. Your machine is the node — it holds your keys, signs your posts, stores your data, and talks straight to your friends' devices.",
            );
            about_item(
                ui,
                theme,
                icons::BADGE,
                "You own your identity",
                "Your identity is a self-sovereign did:key — a keypair only your device holds. No email, no phone number, no account on someone's server. It signs everything you create so others can verify it's really you.",
            );
            about_item(
                ui,
                theme,
                icons::LOCK,
                "Private by cryptography",
                "Messages and media are end-to-end encrypted with post-quantum crypto (ML-KEM-768 + X25519, ChaCha20-Poly1305). Even relays only ever see ciphertext — never your content.",
            );

            // Live network-mode item with a status chip.
            let (chip_txt, chip_col, body) = if !online {
                ("○ Connecting", theme.muted, "Connecting to the carrier…")
            } else if direct {
                ("● Direct P2P", theme.good, "Right now your device is connected DIRECTLY — data flows device-to-device and the relay is only used to introduce peers.")
            } else {
                ("● Relay-assisted", theme.gold_ink, "Right now data rides the encrypted relay (this network blocks a direct link). It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link.")
            };
            about_item_live(ui, theme, icons::SWAP_HORIZ, "Peer-to-peer delivery", body, chip_txt, chip_col);

            about_item(
                ui,
                theme,
                icons::SHIELD,
                "Sandboxed & on-device",
                "All your keys and data live in Hey's private app storage, sandboxed by the OS so other apps can't read them. Nothing is uploaded to a company.",
            );
            about_item(
                ui,
                theme,
                icons::PUBLIC,
                "No lock-in",
                "hey-core is the same engine across phone, web and desktop, speaking open Elastos interfaces. Your identity and social graph are yours to take anywhere.",
            );

            ui.add_space(8.0);
            ui.label(
                RichText::new("hey-core · Elastos Carrier (iroh) · IPFS content store · did:key identity")
                    .size(11.0)
                    .color(theme.muted),
            );
        });
    if !open {
        app.state.modal = None;
    }
}

fn about_item(ui: &mut egui::Ui, theme: &Theme, glyph: &str, title: &str, body: &str) {
    ui.horizontal_top(|ui| {
        gradient_icon(ui, glyph, 34.0);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(15.0).family(icons::semibold()).color(theme.ink));
            ui.label(RichText::new(body).size(13.0).color(theme.muted));
        });
    });
    ui.add_space(7.0);
}

#[allow(clippy::too_many_arguments)]
fn about_item_live(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: &str,
    title: &str,
    body: &str,
    chip: &str,
    chip_col: Color32,
) {
    ui.horizontal_top(|ui| {
        gradient_icon(ui, glyph, 34.0);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(15.0).family(icons::semibold()).color(theme.ink));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    super::chip(ui, theme, chip, chip_col);
                });
            });
            ui.label(RichText::new(body).size(13.0).color(theme.muted));
        });
    });
    ui.add_space(7.0);
}

