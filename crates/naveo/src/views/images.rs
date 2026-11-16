//! Vue Images.

use eframe::egui;
use naveo_core::Command;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let images: Vec<_> = app
        .images
        .clone()
        .into_iter()
        .filter(|i| {
            search.is_empty()
                || i.tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&search))
        })
        .collect();

    let total_bytes: u64 = images.iter().map(|i| i.size_bytes).sum();

    page_header(
        ui,
        &p,
        "Images",
        &format!("{} image(s) · {}", images.len(), theme::human_size(total_bytes)),
    );

    if !app.docker_ok {
        ui.colored_label(p.warn, "Docker n'est pas joignable.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("images-grid")
                .num_columns(4)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    // En-tête
                    ui.strong("Image");
                    ui.strong("Taille");
                    ui.strong("Créée");
                    ui.strong("");
                    ui.end_row();

                    for img in &images {
                        let tag = img.tags.first().cloned().unwrap_or_else(|| "<sans tag>".into());
                        let extra = img.tags.len().saturating_sub(1);

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&tag).strong());
                            if extra > 0 {
                                ui.label(
                                    egui::RichText::new(format!("+{extra}"))
                                        .size(11.0)
                                        .color(p.dim),
                                );
                            }
                        });
                        ui.label(theme::human_size(img.size_bytes));
                        ui.label(
                            egui::RichText::new(format!("il y a {}", days_ago(img.created)))
                                .color(p.dim),
                        );
                        if ui.button("🗑").on_hover_text("Supprimer").clicked() {
                            let target = if tag.contains('@') || tag == "<sans tag>" {
                                img.id.clone()
                            } else {
                                tag.clone()
                            };
                            app.send(Command::RemoveImage(target));
                        }
                        ui.end_row();
                    }
                });
        });
    });
}

fn days_ago(unix_secs: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = (now - unix_secs) / 86_400;
    match days {
        0 => "aujourd'hui".into(),
        1 => "1 jour".into(),
        n => format!("{n} jours"),
    }
}
