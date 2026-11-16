//! Vues de l'application, une par page de la sidebar.

pub mod containers;
pub mod detail;
pub mod images;
pub mod k8s;
pub mod logs;
pub mod networks;
pub mod settings;
pub mod terminal;
pub mod volumes;

use eframe::egui;

use crate::theme::Palette;

/// En-tête de page commun.
pub fn page_header(ui: &mut egui::Ui, p: &Palette, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(22.0).strong());
        ui.label(egui::RichText::new(subtitle).size(13.0).color(p.dim));
    });
    ui.add_space(6.0);
}

/// Bouton « copier au presse-papier ». Renvoie true si cliqué.
pub(crate) fn copy_btn(ui: &mut egui::Ui, text: &str) -> bool {
    if ui.button("📋").on_hover_text("Copier").clicked() {
        ui.ctx().copy_text(text.to_string());
        return true;
    }
    false
}

/// En-tête de colonne triable : cliquer pour trier / inverser.
/// `sort` est l'état (colonne, croissant) de la vue.
pub(crate) fn sort_header(ui: &mut egui::Ui, sort: &mut (usize, bool), col: usize, label: &str) {
    let arrow = if sort.0 == col {
        if sort.1 {
            " ▲"
        } else {
            " ▼"
        }
    } else {
        ""
    };
    if ui
        .button(egui::RichText::new(format!("{label}{arrow}")).strong())
        .clicked()
    {
        if sort.0 == col {
            sort.1 = !sort.1;
        } else {
            *sort = (col, true);
        }
    }
}

/// Filtre de logs par niveau : garde les lignes contenant un marqueur
/// de niveau si un niveau est choisi.
pub(crate) fn filter_log_level(text: &str, level: &str) -> String {
    if level.is_empty() {
        return text.to_string();
    }
    let (want, markers): (&str, Vec<&str>) = match level {
        "error" => (
            "error",
            vec!["ERROR", "Error", "error", "ERR", "FATAL", "panic"],
        ),
        "warn" => ("warn", vec!["WARN", "Warn", "warn", "WARNING"]),
        "info" => ("info", vec!["INFO", "Info", "info", "NOTICE"]),
        "debug" => ("debug", vec!["DEBUG", "Debug", "debug", "TRACE"]),
        _ => ("", vec![]),
    };
    if want.is_empty() {
        return text.to_string();
    }
    text.lines()
        .filter(|l| markers.iter().any(|m| l.contains(m)))
        .collect::<Vec<_>>()
        .join("\n")
}
