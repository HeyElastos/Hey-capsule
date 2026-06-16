//! First-run welcome / onboarding — a desktop adaptation of the Android
//! `WelcomeFlow`: the same 3-page intro (Hey / "Yours, end to end" / "Powered by
//! ElastOS") and the create-new vs restore-from-phrase choice, presented as a
//! single frosted card centered in the window (instead of a full-screen phone
//! pager). Shown until the user chooses, so a restore can supply the seed.

use egui::{Align, Align2, Color32, FontId, RichText, Sense};

use crate::app::App;
use crate::icons;
use crate::theme::{Theme, GOLD, LIKE};

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let avail = ui.available_size();
    let card_w = (avail.x - 48.0).clamp(360.0, 480.0);
    // A FIXED-size, statically-centered card (no per-frame measuring) so it never
    // moves or resizes between pages — only the page text + emoji slide inside it.
    let inner_h = (avail.y - 96.0).clamp(470.0, 560.0);
    let visual_h = inner_h + 40.0; // + the sheet's inner margins
    let top = ((avail.y - visual_h) * 0.5).max(8.0);
    let left = ((avail.x - card_w) * 0.5).max(0.0);

    ui.add_space(top);
    ui.horizontal(|ui| {
        ui.add_space(left);
        theme.sheet().show(ui, |ui| {
            ui.set_width(card_w);
            // STABILITY #14: wrap the (fixed-height) page body in a ScrollArea so a
            // window resized below the clamp floor scrolls instead of clipping.
            egui::ScrollArea::vertical()
                .max_height(inner_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.set_min_height(inner_h);
                    if app.state.onboarding.profile_setup {
                        profile_setup_card(app, ui, theme);
                    } else if app.state.onboarding.restore_mode {
                        restore_card(app, ui, theme);
                    } else {
                        welcome_card(app, ui, theme);
                    }
                });
        });
    });
}

// ── the intro + create/restore choice ─────────────────────────────────────────
fn welcome_card(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        page_content(ui, theme, app.state.onboarding.page.min(2));
        ui.add_space(20.0);
        pager_controls(app, ui, theme);
        ui.add_space(18.0);

        if super::primary_button(ui, true, "Create new identity").clicked() {
            // The runtime auto-created a fresh identity at boot; before entering the
            // app, run the one-time profile-setup step (== Android OnboardingScreen).
            app.begin_profile_setup();
        }
        ui.add_space(10.0);
        if super::outline_button(ui, theme, true, &format!("{}   I have a recovery phrase", icons::KEY)).clicked() {
            app.state.onboarding.restore_mode = true;
            app.state.onboarding.error.clear();
        }

        ui.add_space(16.0);
        theme_toggle_row(app, ui, theme);
        ui.add_space(2.0);
    });
}

/// The intro pager — a fixed-height window with a smooth horizontal SLIDE between
/// pages (the page eases in from the side the arrow/dot moved).
fn page_content(ui: &mut egui::Ui, theme: &Theme, page: usize) {
    let w = ui.available_width();
    let win_h = 300.0;
    // Reserve a FIXED window in the card; its size never changes between pages.
    let (window, _) = ui.allocate_exact_size(egui::vec2(w, win_h), Sense::hover());

    // Animate a float toward the integer page; the lag drives the slide offset.
    let anim = ui.ctx().animate_value_with_time(egui::Id::new("welcome-pager"), page as f32, 0.30);
    let dx = (page as f32 - anim) * (w * 0.55);
    let target = window.translate(egui::vec2(dx, 0.0));

    // Render the page in a NON-allocating child UI so the slide offset (and each
    // page's own content height) never grow the card — only the content moves.
    let mut child = ui.child_ui(target, egui::Layout::top_down(Align::Center), None);
    child.set_clip_rect(window);
    child.set_width(w);
    render_page(&mut child, theme, page);

    if (anim - page as f32).abs() > 0.003 {
        ui.ctx().request_repaint();
    }
}

fn render_page(ui: &mut egui::Ui, theme: &Theme, page: usize) {
    match page {
        0 => {
            // gold radial glow + the waving-hand mark (same as Android)
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(132.0, 132.0), Sense::hover());
            let c = rect.center();
            let p = ui.painter();
            for i in (0..14).rev() {
                let t = i as f32 / 14.0;
                let r = 66.0 * (0.3 + 0.7 * t);
                let a = (1.0 - t).powf(2.0) * 0.30; // soften the hero glow peak (the ONE allowed glow)
                p.circle_filled(c, r, GOLD.gamma_multiply(a));
            }
            p.text(c, Align2::CENTER_CENTER, "👋", FontId::proportional(58.0), theme.ink);
            ui.add_space(2.0);
            ui.label(RichText::new("Hey").size(48.0).family(icons::display()).color(theme.gold_ink));
            ui.add_space(4.0);
            ctext(ui, "a warm little corner of the internet that's truly yours", 15.5, theme.ink, true);
            ui.add_space(10.0);
            ctext(ui, "No ads, no snooping, no strangers in your data — just you and the people you love, safe on your own device.", 13.5, theme.muted, false);
        }
        1 => {
            ui.add_space(34.0);
            ui.label(RichText::new("Yours, end to end").size(22.0).family(icons::semibold()).color(theme.ink));
            ui.add_space(16.0);
            theme.glass(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                onb_row(ui, theme, icons::KEY, "A self-sovereign identity", "A did:key generated and held only on this device.");
                ui.add_space(12.0);
                onb_row(ui, theme, icons::LOCK, "End-to-end encrypted", "Post-quantum DMs + signed posts. No middleman can read them.");
                ui.add_space(12.0);
                onb_row(ui, theme, icons::CLOUD_OFF, "No servers, no accounts", "Your data lives with you and your friends — nowhere else.");
            });
        }
        _ => {
            ui.add_space(56.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 210.0).max(0.0) * 0.5);
                ui.label(RichText::new(icons::PUBLIC).size(24.0).color(theme.gold_ink));
                ui.add_space(8.0);
                ui.label(RichText::new("Powered by ElastOS").size(18.0).family(icons::semibold()).color(theme.ink));
            });
            ui.add_space(14.0);
            ctext(ui, "ElastOS is a decentralized internet where you — not companies — own your identity, data, and money. This device is the node. One recovery phrase is your sovereign identity and wallet across the whole network.", 13.5, theme.muted, false);
        }
    }
}

/// Centered, wrapping text label (Android centers its onboarding copy).
fn ctext(ui: &mut egui::Ui, text: &str, size: f32, color: Color32, strong: bool) {
    let mut rt = RichText::new(text).size(size).color(color);
    if strong {
        rt = rt.strong();
    }
    ui.add(egui::Label::new(rt).halign(Align::Center));
}

fn onb_row(ui: &mut egui::Ui, theme: &Theme, icon: &str, title: &str, desc: &str) {
    ui.horizontal_top(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new(icon).size(22.0).color(theme.gold_ink));
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(15.0).family(icons::semibold()).color(theme.ink));
            ui.add(egui::Label::new(RichText::new(desc).size(12.5).color(theme.muted)).wrap());
        });
    });
}

fn pager_controls(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let avail = ui.available_width();
    let row_w = 3.0 * 18.0 + 2.0 * 36.0;
    ui.horizontal(|ui| {
        ui.add_space(((avail - row_w) * 0.5).max(0.0));
        if app.state.onboarding.page > 0 {
            if super::icon_button(ui, theme, icons::ARROW_BACK, 18.0, theme.muted).clicked() {
                app.state.onboarding.page -= 1;
            }
        } else {
            ui.add_space(36.0);
        }
        for i in 0..3usize {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::click());
            let sel = i == app.state.onboarding.page;
            let col = if sel { theme.gold_ink } else { theme.muted.gamma_multiply(0.4) };
            ui.painter().circle_filled(r.center(), if sel { 4.5 } else { 3.5 }, col);
            if resp.clicked() {
                app.state.onboarding.page = i;
            }
        }
        if app.state.onboarding.page < 2 {
            if super::icon_button(ui, theme, icons::CHEVRON_RIGHT, 18.0, theme.muted).clicked() {
                app.state.onboarding.page += 1;
            }
        } else {
            ui.add_space(36.0);
        }
    });
}

fn theme_toggle_row(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // A centered Light/Dark segmented control (sliding thumb). 0 = Light, 1 = Dark.
    let seg_w = 200.0;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - seg_w) * 0.5).max(0.0));
        ui.allocate_ui(egui::vec2(seg_w, 34.0), |ui| {
            let selected = if app.state.light { 0 } else { 1 };
            if let Some(i) = super::segmented(ui, theme, "welcome-theme", &["Light", "Dark"], selected) {
                app.state.light = i == 0;
                crate::theme::save_pref(app.state.light); // persist across launches
            }
        });
    });
}

// ── post-create profile setup (== Android OnboardingScreen) ───────────────────
//
// CREATE-new only. Shown once, right after the identity is created and before the
// app opens. Mirrors Android: a "👋 Set up your profile" hero, a tappable avatar
// (pick → scale → upload, reusing `app.pick_avatar`), a nickname field, an optional
// bio, then Continue → `set_profile` (the SAME engine fn the EditProfile sheet uses)
// and into the app. There is no Skip (Android has none) — a blank nickname falls
// back to "Hey user", exactly like Android.
fn profile_setup_card(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);

        // 👋 hero with the gold radial glow (matches the welcome page-0 mark).
        let (rect, _) = ui.allocate_exact_size(egui::vec2(96.0, 96.0), Sense::hover());
        let c = rect.center();
        let p = ui.painter();
        for i in (0..14).rev() {
            let t = i as f32 / 14.0;
            let r = 48.0 * (0.3 + 0.7 * t);
            let a = (1.0 - t).powf(2.0) * 0.30;
            p.circle_filled(c, r, GOLD.gamma_multiply(a));
        }
        p.text(c, Align2::CENTER_CENTER, "👋", FontId::proportional(46.0), theme.ink);

        ui.add_space(10.0);
        ui.label(RichText::new("Set up your profile").size(22.0).family(icons::semibold()).color(theme.ink));
        ui.add_space(4.0);
        ctext(ui, "This is how others will see you. You can change it anytime.", 13.0, theme.muted, false);
        ui.add_space(18.0);

        // Tappable avatar (96) — pick → scale → upload (reuses app.pick_avatar, the
        // SAME helper the EditProfile sheet uses). A picked CID renders via the media
        // cache; otherwise a gold gradient tile with an add-photo glyph.
        let cid = app.state.profile_draft.avatar_cid.clone();
        let tex = if cid.is_empty() {
            None
        } else {
            app.media.texture(&cid, &app.engine, &app.ev_tx)
        };
        let resp = if let Some(t) = tex {
            ui.add(
                egui::Image::new(egui::load::SizedTexture::from_handle(&t))
                    .fit_to_exact_size(egui::Vec2::splat(96.0))
                    .rounding(48.0)
                    .sense(Sense::click()),
            )
        } else {
            let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(96.0), Sense::click());
            let pr = ui.painter();
            pr.circle_filled(r.center(), 48.0, theme.gold_ink.gamma_multiply(0.45));
            pr.circle_filled(r.center() + egui::vec2(0.0, -10.0), 44.0, GOLD);
            pr.text(r.center(), Align2::CENTER_CENTER, icons::ADD_A_PHOTO, FontId::proportional(30.0), crate::theme::NAVY);
            resp
        };
        if resp.clicked() {
            app.pick_avatar();
        }

        ui.add_space(16.0);
        super::field(ui, theme, &mut app.state.profile_draft.nickname, "Nickname", 1);
        ui.add_space(10.0);
        super::field(ui, theme, &mut app.state.profile_draft.bio, "Short bio (optional)", 3);

        ui.add_space(14.0);
        // Privacy reassurance card (matches Android's "Your data stays on this device").
        theme.glass(14.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icons::SHIELD).size(20.0).color(theme.good));
                ui.add_space(8.0);
                ui.label(RichText::new("Your data stays on this device").size(14.0).family(icons::semibold()).color(theme.ink));
            });
            ui.add_space(6.0);
            ui.add(egui::Label::new(
                RichText::new("Hey is sandboxed and stored only on your machine. Nothing is uploaded to a company.")
                    .size(12.5)
                    .color(theme.muted),
            ).wrap());
        });

        ui.add_space(20.0);
        let busy = app.state.profile_draft.busy;
        if busy {
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 160.0).max(0.0) * 0.5);
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new("Setting up…").size(13.0).color(theme.gold_ink));
            });
        } else if super::primary_button(ui, true, "Continue").clicked() {
            app.submit_onboard_profile();
        }
        ui.add_space(2.0);
    });
}

// ── restore from recovery phrase ──────────────────────────────────────────────
fn restore_card(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal(|ui| {
        if super::icon_button(ui, theme, icons::ARROW_BACK, 20.0, theme.ink).clicked() {
            app.state.onboarding.restore_mode = false;
            app.state.onboarding.error.clear();
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Restore your account").size(20.0).family(icons::semibold()).color(theme.ink));
    });
    ui.add_space(10.0);
    ui.add(
        egui::Label::new(
            RichText::new("Enter your 12-word Hey recovery phrase. It re-derives your identity, your Elastos DID and your wallets on this device — nothing is uploaded.")
                .size(13.5)
                .color(theme.muted),
        )
        .wrap(),
    );
    ui.add_space(16.0);

    let busy = app.state.onboarding.busy;
    ui.add_enabled(
        !busy,
        egui::TextEdit::multiline(&mut app.state.onboarding.phrase)
            .desired_width(ui.available_width())
            .desired_rows(4)
            .margin(egui::Margin::symmetric(14.0, 12.0))
            .font(egui::FontId::proportional(17.0))
            .hint_text("word1  word2  word3  …"),
    );

    if !app.state.onboarding.error.is_empty() {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(&app.state.onboarding.error).size(13.0).color(LIKE)).wrap());
    }

    ui.add_space(16.0);
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.add_space(8.0);
            ui.label(RichText::new("Restoring… the app will restart.").size(13.0).color(theme.gold_ink));
        });
    } else if super::primary_button(ui, true, "Restore").clicked() {
        let p = app
            .state
            .onboarding
            .phrase
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if hey_mobile_runtime::validate_mnemonic(&p) {
            app.state.onboarding.error.clear();
            app.state.onboarding.busy = true;
            app.onboard_restore(p);
        } else {
            app.state.onboarding.error =
                "That doesn't look like a valid 12-word recovery phrase. Check the words, spelling and order.".into();
        }
    }

    ui.add_space(12.0);
    ui.add(
        egui::Label::new(
            RichText::new("It's the same 12 words you can import into official Elastos Essentials.")
                .size(12.0)
                .color(theme.muted),
        )
        .wrap(),
    );
}
