//! Verse tab — a launch panel for the REAL Godot Hey Verse. The desktop runs the
//! exact same Godot project as mobile (bit-for-bit: same scenes, scripts, models),
//! just in a landscape window, via the engine bundled with the app (no download).
//! Because it's the same game on every device, items the user owns (furniture, a
//! kitchen, …) carry across mobile and desktop.

use crate::app::App;
use crate::icons;
use crate::theme::Theme;

use egui::RichText;

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let avail = ui.available_width();
    let col_w = avail.min(620.0);
    let pad = ((avail - col_w) * 0.5).max(0.0);

    ui.add_space(48.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(col_w);
            theme.glass(18.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(icons::PUBLIC).size(42.0).color(theme.gold_ink));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Hey Verse")
                            .size(26.0)
                            .family(icons::display())
                            .color(theme.ink),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Your sovereign 3D home. The same world as on your phone — walk \
                             around, decorate it, and everything you own carries across every \
                             device you sign in on.",
                        )
                        .size(13.0)
                        .color(theme.muted),
                    );
                    ui.add_space(18.0);
                    if crate::views::primary_button(
                        ui,
                        true,
                        &format!("{}  Open Hey Verse", icons::PUBLIC),
                    )
                    .clicked()
                    {
                        let now = ui.ctx().input(|i| i.time);
                        app.launch_verse(now);
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Opens in a landscape window · engine bundled in the app, nothing to download")
                            .size(11.0)
                            .color(theme.muted),
                    );
                    ui.add_space(10.0);
                });
            });
        });
    });
}
