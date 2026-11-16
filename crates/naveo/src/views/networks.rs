//! Vue Réseaux.

use eframe::egui;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let networks: Vec<_> = app
        .networks
        .clone()
        .into_iter()
        .filter(|n| search.is_empty() || n.name.to_lowercase().contains(&search))
        .collect();

    page_header(ui, &p, "Réseaux", &format!("{} réseau(x)", networks.len()));

    if !app.docker_ok {
        ui.colored_label(p.warn, "Docker n'est pas joignable.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("networks-grid")
                .num_columns(3)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Nom");
                    ui.strong("Driver");
                    ui.strong("Portée");
                    ui.end_row();

                    for n in &networks {
                        ui.label(egui::RichText::new(&n.name).strong());
                        ui.label(&n.driver);
                        ui.label(egui::RichText::new(&n.scope).color(p.dim));
                        ui.end_row();
                    }
                });
        });
    });
}
