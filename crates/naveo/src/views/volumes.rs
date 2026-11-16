//! Vue Volumes.

use eframe::egui;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let volumes: Vec<_> = app
        .volumes
        .clone()
        .into_iter()
        .filter(|v| search.is_empty() || v.name.to_lowercase().contains(&search))
        .collect();

    page_header(ui, &p, "Volumes", &format!("{} volume(s)", volumes.len()));

    if !app.docker_ok {
        ui.colored_label(p.warn, "Docker n'est pas joignable.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("volumes-grid")
                .num_columns(3)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Nom");
                    ui.strong("Driver");
                    ui.strong("Point de montage");
                    ui.end_row();

                    for v in &volumes {
                        ui.label(egui::RichText::new(&v.name).strong());
                        ui.label(&v.driver);
                        ui.label(egui::RichText::new(&v.mountpoint).color(p.dim));
                        ui.end_row();
                    }
                });
        });
    });
}
