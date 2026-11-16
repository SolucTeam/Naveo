//! Vues de l'application, une par page de la sidebar.

pub mod containers;
pub mod images;
pub mod k8s;
pub mod logs;
pub mod networks;
pub mod settings;
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
