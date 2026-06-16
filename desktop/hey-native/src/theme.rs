//! Production design system — "Aurora": a modern, high-quality iPadOS look.
//!
//! Cool-neutral light + graphite dark surfaces; GOLD is the lone chroma accent
//! (primary action, selection pill, focus ring, self-identity) — never decoration,
//! never system-blue. Cards are OPAQUE material tokens; depth comes from tone steps
//! (window → sidebar → card) + soft shadows reserved for things that truly float
//! (sheets, menus, dock, FAB). No backdrop blur (egui can't sample behind the
//! window); "materials" = opaque tonal surfaces + hairlines + layered tone steps.
//!
//! The Theme struct keeps the field/method names the views already call, so the
//! whole app re-skins by changing values here. `NAVY` is kept as an alias for the
//! on-gold ink so every existing call site compiles unchanged.

use egui::{Color32, Frame, Margin, Rect, Stroke, Vec2};

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

// ── brand constants ──────────────────────────────────────────────────────────
/// The primary accent — Hey warm gold. Logo + buttons + selection + focus ring.
pub const GOLD: Color32 = Color32::from_rgb(0xD4, 0xB8, 0x4B);
/// Hover / pressed gold only.
pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(0xE0, 0xC6, 0x66);
/// Avatar gradient bright stop (calmer than the old neon #FACC15).
pub const GOLD2: Color32 = Color32::from_rgb(0xE8, 0xCF, 0x6B);
/// Destructive + like-active only (modern red).
pub const LIKE: Color32 = Color32::from_rgb(0xE2, 0x57, 0x4C);
/// Ink that sits ON a gold fill (button labels). Intent is ON_GOLD; the name
/// `NAVY` is KEPT as an alias so every existing `color(NAVY)` call compiles
/// unchanged. It is now a warm near-black, not a blue.
pub const NAVY: Color32 = Color32::from_rgb(0x1A, 0x16, 0x05);

#[derive(Clone, Copy)]
pub struct Theme {
    pub light: bool,
    // background canvas (subtle 1-stop fade bg1 -> bg3; bg2 = sidebar/dock material)
    pub bg1: Color32,
    pub bg2: Color32,
    pub bg3: Color32,
    // text
    pub ink: Color32,   // text.high
    pub muted: Color32, // text.mid
    pub faint: Color32, // text.low / placeholder
    // surfaces
    pub glass_fill: Color32,   // grouped card/row fill (OPAQUE material)
    pub glass_border: Color32, // hairline border / separators
    pub border_strong: Color32,
    pub sheet_bg: Color32, // sheets / modals (one step above cards)
    pub surface2: Color32, // menus / hovered material / floating
    pub hover: Color32,    // neutral hover wash
    // accents-as-color
    pub gold_ink: Color32,
    pub good: Color32,   // success / online
    // chat bubbles
    pub bubble_in: Color32, // bubble.them
    pub bubble_me: Color32, // bubble.me
}

impl Theme {
    pub fn get(light: bool) -> Theme {
        if light {
            Theme::light()
        } else {
            Theme::dark()
        }
    }

    // Graphite "system-grouped" dark.
    fn dark() -> Theme {
        Theme {
            light: false,
            bg1: rgb(0x0E, 0x0E, 0x12),
            bg2: rgb(0x16, 0x18, 0x1D),
            bg3: rgb(0x0C, 0x0C, 0x10),
            ink: rgb(0xF2, 0xF2, 0xF5),
            muted: rgb(0x9A, 0x9A, 0xA4),
            faint: rgb(0x5E, 0x5E, 0x68),
            glass_fill: rgb(0x1C, 0x1C, 0x1F),
            glass_border: rgba(0xFF, 0xFF, 0xFF, 18),
            border_strong: rgba(0xFF, 0xFF, 0xFF, 36),
            sheet_bg: rgb(0x23, 0x23, 0x2A),
            surface2: rgb(0x26, 0x26, 0x2D),
            hover: rgba(0xFF, 0xFF, 0xFF, 14),
            gold_ink: rgb(0xE0, 0xC6, 0x6A),
            good: rgb(0x30, 0xD1, 0x58),
            bubble_in: rgb(0x2A, 0x2A, 0x32),
            bubble_me: rgb(0x6B, 0x5A, 0x24),
        }
    }

    // Cool-neutral "systemGroupedBackground" light.
    fn light() -> Theme {
        Theme {
            light: true,
            bg1: rgb(0xF2, 0xF2, 0xF7),
            bg2: rgb(0xFB, 0xFB, 0xFD),
            bg3: rgb(0xEC, 0xEC, 0xF1),
            ink: rgb(0x1C, 0x1C, 0x1E),
            muted: rgb(0x6E, 0x6E, 0x73),
            faint: rgb(0xAE, 0xAE, 0xB2),
            glass_fill: rgb(0xFF, 0xFF, 0xFF),
            glass_border: rgba(0x3C, 0x3C, 0x43, 32),
            border_strong: rgba(0x3C, 0x3C, 0x43, 56),
            sheet_bg: rgb(0xFF, 0xFF, 0xFF),
            surface2: rgb(0xFF, 0xFF, 0xFF),
            hover: rgba(0x3C, 0x3C, 0x43, 12),
            gold_ink: rgb(0x8A, 0x6D, 0x12),
            good: rgb(0x28, 0xA7, 0x45),
            bubble_in: rgb(0xFF, 0xFF, 0xFF),
            bubble_me: rgb(0xEF, 0xCE, 0x6B),
        }
    }

    /// Push the palette + smooth widget styling into egui's global Style/Visuals.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.override_text_color = Some(self.ink);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = self.sheet_bg;
        v.window_stroke = Stroke::new(1.0, self.glass_border);
        v.window_rounding = egui::Rounding::same(20.0); // sheet corners
        v.menu_rounding = egui::Rounding::same(14.0);
        v.extreme_bg_color = if self.light {
            rgb(0xEC, 0xEC, 0xF1) // recessed grey so fields are VISIBLE on white sheets/cards
        } else {
            rgb(0x0C, 0x0C, 0x10) // recessed dark so fields stand out from cards
        }; // text-edit bg
        v.hyperlink_color = self.gold_ink;
        v.selection.bg_fill = GOLD.gamma_multiply(0.22);
        v.selection.stroke = Stroke::new(1.0, GOLD);

        // Smooth, neutral widget styling (egui animates between these on hover).
        let r = egui::Rounding::same(12.0);
        let field_bg = v.extreme_bg_color; // keep the focused-field interior neutral
        let w = &mut v.widgets;
        w.noninteractive.rounding = r;
        w.noninteractive.bg_stroke = Stroke::new(1.0, self.glass_border);
        w.inactive.rounding = r;
        w.inactive.weak_bg_fill = self.glass_fill;
        w.inactive.bg_fill = self.glass_fill;
        w.inactive.bg_stroke = Stroke::new(1.0, self.glass_border);
        w.inactive.fg_stroke = Stroke::new(1.0, self.ink);
        w.hovered.rounding = r;
        w.hovered.weak_bg_fill = self.surface2;
        w.hovered.bg_fill = self.surface2;
        w.hovered.bg_stroke = Stroke::new(1.0, self.border_strong);
        w.hovered.fg_stroke = Stroke::new(1.0, self.ink);
        w.hovered.expansion = 1.0;
        w.active.rounding = r;
        w.active.weak_bg_fill = field_bg;
        w.active.bg_fill = field_bg;
        w.active.bg_stroke = Stroke::new(1.5, GOLD); // FOCUS RING = 1.5px gold
        w.active.fg_stroke = Stroke::new(1.0, self.ink);
        w.active.expansion = 0.0;
        v.text_cursor.stroke = Stroke::new(2.0, GOLD); // gold caret
        v.interact_cursor = Some(egui::CursorIcon::PointingHand);
        ctx.set_visuals(v);

        let mut style = (*ctx.style()).clone();
        style.interaction.selectable_labels = false;
        style.spacing.button_padding = egui::vec2(16.0, 10.0); // 8-pt, roomier
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.interact_size = egui::vec2(44.0, 42.0); // taller, comfortable fields/buttons
        style.spacing.window_margin = Margin::same(0.0);
        style.animation_time = 0.16; // smooth springs
        // Comfortable, legible type scale — `Body` drives every text FIELD, so a
        // bigger Body is what makes inputs read as proper social-app text boxes.
        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::new(22.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Body, egui::FontId::new(16.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Button, egui::FontId::new(16.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Small, egui::FontId::new(12.5, egui::FontFamily::Proportional)),
            (egui::TextStyle::Monospace, egui::FontId::new(13.5, egui::FontFamily::Monospace)),
        ]
        .into();
        ctx.set_style(style);
    }

    /// A CLEARLY VISIBLE text-field fill (distinct from white sheets / cards) so an
    /// input reads as an obvious box you can see — paired with `field_border`.
    pub fn field_fill(&self) -> Color32 {
        if self.light {
            rgb(0xE9, 0xE9, 0xF0)
        } else {
            rgb(0x0C, 0x0C, 0x10)
        }
    }
    pub fn field_border(&self) -> Color32 {
        self.border_strong
    }

    /// iPadOS inset-grouped material: opaque fill, hairline, big radius. A faint
    /// lift only in dark. Reserve real shadows for things that float.
    pub fn glass(&self, radius: f32) -> Frame {
        Frame::none()
            .fill(self.glass_fill) // OPAQUE material
            .stroke(Stroke::new(1.0, self.glass_border)) // hairline both themes
            .rounding(radius.max(14.0))
            .inner_margin(Margin::same(16.0))
            .shadow(if self.light {
                egui::epaint::Shadow::NONE
            } else {
                egui::epaint::Shadow {
                    offset: Vec2::new(0.0, 1.0),
                    blur: 3.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(40),
                }
            })
    }

    /// The few cards that should genuinely pop (hero, balance) — one soft shadow.
    pub fn material_raised(&self, radius: f32) -> Frame {
        self.glass(radius).shadow(egui::epaint::Shadow {
            offset: Vec2::new(0.0, 2.0),
            blur: if self.light { 12.0 } else { 16.0 },
            spread: 0.0,
            color: Color32::from_black_alpha(if self.light { 12 } else { 60 }),
        })
    }

    /// An ELEVATED floating surface — menus, the emoji picker, popups.
    pub fn floating(&self, radius: f32) -> Frame {
        Frame::none()
            .fill(self.surface2)
            .stroke(Stroke::new(1.0, self.border_strong))
            .rounding(radius)
            .inner_margin(Margin::same(12.0))
            .shadow(egui::epaint::Shadow {
                offset: Vec2::new(0.0, 8.0),
                blur: 24.0,
                spread: 0.0,
                color: Color32::from_black_alpha(if self.light { 26 } else { 90 }),
            })
    }

    /// An iPad sheet/dialog container — opaque material, grabber (see
    /// `sheet_handle`), radius 20, soft lift.
    pub fn sheet(&self) -> Frame {
        Frame::none()
            .fill(self.sheet_bg) // opaque material
            .rounding(20.0)
            .inner_margin(Margin {
                left: 22.0,
                right: 22.0,
                top: 14.0,
                bottom: 22.0,
            })
            .stroke(if self.light {
                Stroke::new(1.0, self.glass_border)
            } else {
                Stroke::NONE
            })
            .shadow(egui::epaint::Shadow {
                offset: Vec2::new(0.0, 16.0),
                blur: 44.0,
                spread: 0.0,
                color: Color32::from_black_alpha(if self.light { 48 } else { 150 }),
            })
    }

    /// Call at the top of any sheet body — the iPad grab handle.
    pub fn sheet_handle(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(36.0, 5.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(r, 2.5, self.faint.gamma_multiply(0.5));
        });
        ui.add_space(10.0);
    }

    /// The full-height left navigation sidebar — opaque tonal material anchored to
    /// the window edge. The panel's separator line distinguishes it from content.
    pub fn sidebar_frame(&self) -> Frame {
        Frame::none()
            .fill(self.bg2) // opaque; the separator line distinguishes it
            .inner_margin(Margin {
                left: 16.0,
                right: 14.0,
                top: crate::app::TOP_INSET + 14.0,
                bottom: 16.0,
            })
    }

    /// Paint the calm canvas: a single flat material fill with one barely-there
    /// vertical tone step (bg1 → bg3). No glow blobs — depth comes from the tone
    /// step window(bg1) -> sidebar(bg2) -> card(glass_fill).
    pub fn paint_background(&self, painter: &egui::Painter, rect: Rect) {
        use egui::epaint::{Mesh, Vertex, WHITE_UV};
        let mut m = Mesh::default();
        let push = |m: &mut Mesh, p: egui::Pos2, c: Color32| {
            m.vertices.push(Vertex {
                pos: p,
                uv: WHITE_UV,
                color: c,
            })
        };
        push(&mut m, rect.left_top(), self.bg1);
        push(&mut m, rect.right_top(), self.bg1);
        push(&mut m, rect.left_bottom(), self.bg3);
        push(&mut m, rect.right_bottom(), self.bg3);
        m.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
        painter.add(egui::Shape::mesh(m));
        // NO glow() calls.
    }
}

/// Linear interpolation between two colors. Exported `pub(crate)` so the shell
/// (`app.rs`) + views can use it for animated tints (rail pill, segmented, etc.).
pub(crate) fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

// ── light/dark persistence ─────────────────────────────────────────────────────
// Android remembers the Light/Dark choice across launches. The desktop mirrors
// that by writing a one-line `theme.txt` ("light"/"dark") in the same data dir
// the runtime uses (`dirs::data_dir()/hey-social-native`). Everything here is
// best-effort: any IO error silently falls back to the caller's default so a
// missing or garbled file never panics or blocks the first frame.

/// Path to the persisted theme file, next to the runtime's data dir.
fn theme_pref_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("hey-social-native").join("theme.txt"))
}

/// Read the saved theme. `Some(true)` = light, `Some(false)` = dark, `None` when
/// nothing valid is stored (caller should keep its own default).
pub fn load_pref() -> Option<bool> {
    let raw = std::fs::read_to_string(theme_pref_path()?).ok()?;
    match raw.trim() {
        "light" => Some(true),
        "dark" => Some(false),
        _ => None,
    }
}

/// Persist the Light/Dark choice. Best-effort; errors are ignored.
pub fn save_pref(light: bool) {
    if let Some(path) = theme_pref_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, if light { "light" } else { "dark" }).ok();
    }
}
