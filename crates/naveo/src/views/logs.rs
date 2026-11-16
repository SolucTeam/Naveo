//! Vue Journal — logs Docker et Kubernetes (inspiré de Headlamp).

use eframe::egui;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::{filter_log_level, page_header};

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
            .width(300.0)
            .show_ui(ui, |ui| {
                for t in &targets {
                    ui.selectable_value(&mut app.log_target, Some(t.clone()), t);
                }
            });

        // Filtre texte + niveau.
        ui.add_sized(
            [200.0, 22.0],
            egui::TextEdit::singleline(&mut app.log_filter).hint_text("🔍 filtrer…"),
        );
        egui::ComboBox::from_id_salt("journal-level")
            .selected_text(if app.log_level.is_empty() {
                "niveau : tous"
            } else {
                app.log_level.as_str()
            })
            .width(110.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.log_level, String::new(), "tous");
                ui.selectable_value(&mut app.log_level, "error".into(), "error");
                ui.selectable_value(&mut app.log_level, "warn".into(), "warn");
                ui.selectable_value(&mut app.log_level, "info".into(), "info");
                ui.selectable_value(&mut app.log_level, "debug".into(), "debug");
            });

        if ui
            .button("💾")
            .on_hover_text("Télécharger les logs")
            .clicked()
        {
            if let Some(dest) = rfd::FileDialog::new()
                .set_title("Sauvegarder les logs")
                .set_file_name("naveo-logs.txt")
                .save_file()
            {
                let text = app.logs.get(&selected).cloned().unwrap_or_default();
                let _ = std::fs::write(dest, text);
            }
        }
    });

    let raw = app.logs.get(&selected).cloned().unwrap_or_default();
    let line_count = raw.lines().count();
    let mut text = raw;
    let filter = app.log_filter.to_lowercase();
    if !filter.is_empty() {
        text = text
            .lines()
            .filter(|l| l.to_lowercase().contains(&filter))
            .collect::<Vec<_>>()
            .join("\n");
    }
    text = filter_log_level(&text, &app.log_level);

    ui.label(
        egui::RichText::new(format!("{selected} · {line_count} ligne(s)"))
            .size(11.0)
            .color(p.dim),
    );
    ui.add_space(4.0);

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
