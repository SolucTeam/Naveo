//! Vue Journal — logs Docker et Kubernetes (inspiré de Headlamp).

use eframe::egui;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;

    let targets: Vec<String> = {
        let mut v: Vec<String> = app.logs.keys().cloned().collect();
        v.sort();
        v
    };

    page_header(ui, &p, "Journal", "logs conteneurs et pods");

    if targets.is_empty() {
        ui.label(
            egui::RichText::new(
                "Aucun log pour l'instant. Ouvre les logs d'un conteneur 📜 ou d'un pod.",
            )
            .color(p.dim),
        );
        return;
    }

    // Sélection de la source.
    let selected = app.log_target.clone().unwrap_or_else(|| targets[0].clone());
    ui.horizontal(|ui| {
        ui.label("Source :");
        egui::ComboBox::from_id_salt("log-source")
            .selected_text(&selected)
            .width(360.0)
            .show_ui(ui, |ui| {
                for t in &targets {
                    ui.selectable_value(&mut app.log_target, Some(t.clone()), t);
                }
            });
        if ui.button("↻ Recharger").clicked() {
            // Relance une récupération : on ne connaît pas le type ici,
            // on renvoie simplement la dernière version connue.
        }
    });
    ui.add_space(8.0);

    let text = app.logs.get(&selected).cloned().unwrap_or_default();

    theme::card(ui, &p, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height().min(520.0));
                ui.add(
                    egui::TextEdit::multiline(&mut text.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(28)
                        .desired_width(ui.available_width())
                        .interactive(true),
                );
            });
    });
}
