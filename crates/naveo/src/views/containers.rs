//! Vue Conteneurs — groupés par projet Compose, façon Kiri.

use std::collections::BTreeMap;

use eframe::egui;
use naveo_core::{Command, ContainerInfo};

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let containers: Vec<ContainerInfo> = app
        .containers
        .clone()
        .into_iter()
        .filter(|c| {
            search.is_empty()
                || c.name.to_lowercase().contains(&search)
                || c.image.to_lowercase().contains(&search)
                || c.project
                    .as_ref()
                    .is_some_and(|pr| pr.to_lowercase().contains(&search))
        })
        .collect();

    page_header(
        ui,
        &p,
        "Conteneurs",
        &format!("{} conteneur(s)", containers.len()),
    );

    if !app.docker_ok {
        ui.colored_label(
            p.warn,
            "Docker n'est pas joignable. Lance un daemon (Docker Desktop, Kiri…) puis ↻.",
        );
        return;
    }

    // Groupement par projet Compose.
    let mut groups: BTreeMap<String, Vec<ContainerInfo>> = BTreeMap::new();
    for c in containers {
        groups
            .entry(c.project.clone().unwrap_or_else(|| "sans projet".into()))
            .or_default()
            .push(c);
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (group, list) in groups {
            theme::card(ui, &p, |ui| {
                let running = list.iter().filter(|c| c.state == "running").count();
                let header = format!("🗂 {}   ·   {}/{} actif(s)", group, running, list.len());

                egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(format!("grid-{group}"))
                            .num_columns(5)
                            .spacing([16.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for c in &list {
                                    container_row(app, ui, c);
                                }
                            });
                    });
            });
            ui.add_space(4.0);
        }
    });
}

fn container_row(app: &mut NaveoApp, ui: &mut egui::Ui, c: &ContainerInfo) {
    let p = app.palette;
    let color = theme::state_color(&p, &c.state);

    // Statut + nom
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(egui::RichText::new(&c.name).strong());
    });

    // Image
    ui.label(egui::RichText::new(&c.image).color(p.dim));

    // Ports
    ui.label(if c.ports.is_empty() {
        "—".to_string()
    } else {
        c.ports.join(", ")
    });

    // Statut lisible
    ui.label(egui::RichText::new(&c.status).color(p.dim));

    // Actions
    ui.horizontal(|ui| {
        let running = c.state == "running";
        if ui.add_enabled(!running, egui::Button::new("▶")).clicked() {
            app.send(Command::StartContainer(c.name.clone()));
        }
        if ui.add_enabled(running, egui::Button::new("⏹")).clicked() {
            app.send(Command::StopContainer(c.name.clone()));
        }
        if ui.button("📜").on_hover_text("Logs").clicked() {
            app.send(Command::ContainerLogs(c.name.clone()));
        }
        if ui.button("⌨").on_hover_text("Terminal").clicked() {
            app.exec_target = Some(format!("docker:{}", c.name));
            app.page = crate::app::Page::Terminal;
        }
        if ui.button("🗑").on_hover_text("Supprimer").clicked() {
            app.send(Command::RemoveContainer(c.name.clone()));
        }
    });

    ui.end_row();
}
