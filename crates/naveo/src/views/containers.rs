//! Vue Conteneurs — groupés par projet Compose, avec actions projet
//! et actions par conteneur.

use std::collections::BTreeMap;

use eframe::egui;
use naveo_core::{Command, ContainerInfo};

use crate::app::{NaveoApp, Page};
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
            "Docker n'est pas joignable. Lance un daemon Docker puis ↻.",
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
                let header = format!("🗂 {group}   ·   {}/{} actif(s)", running, list.len());

                egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        // Actions sur tout le projet Compose.
                        if group != "sans projet" {
                            ui.horizontal(|ui| {
                                if ui
                                    .button("▶ projet")
                                    .on_hover_text("Démarrer tout le projet")
                                    .clicked()
                                {
                                    app.send(Command::ProjectAction {
                                        project: group.clone(),
                                        action: "start".into(),
                                    });
                                }
                                if ui
                                    .button("⏹ projet")
                                    .on_hover_text("Arrêter tout le projet")
                                    .clicked()
                                {
                                    app.send(Command::ProjectAction {
                                        project: group.clone(),
                                        action: "stop".into(),
                                    });
                                }
                                if ui
                                    .button("🔄 projet")
                                    .on_hover_text("Redémarrer tout le projet")
                                    .clicked()
                                {
                                    app.send(Command::ProjectAction {
                                        project: group.clone(),
                                        action: "restart".into(),
                                    });
                                }
                                if ui
                                    .button("🗑 projet")
                                    .on_hover_text("Supprimer tous les conteneurs du projet")
                                    .clicked()
                                {
                                    app.send(Command::ProjectAction {
                                        project: group.clone(),
                                        action: "remove".into(),
                                    });
                                }
                            });
                        }
                        ui.add_space(2.0);

                        egui::Grid::new(format!("grid-{group}"))
                            .num_columns(6)
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

    // Statut + nom (cliquable : ouvre le détail)
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        if ui.link(egui::RichText::new(&c.name).strong()).clicked() {
            app.page = Page::ContainerDetail(c.name.clone());
        }
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
        let paused = c.state == "paused";

        if ui.add_enabled(!running, egui::Button::new("▶")).clicked() {
            app.send(Command::StartContainer(c.name.clone()));
        }
        if ui.add_enabled(running, egui::Button::new("⏹")).clicked() {
            app.send(Command::StopContainer(c.name.clone()));
        }
        if ui
            .add_enabled(running, egui::Button::new("🔄"))
            .on_hover_text("Redémarrer")
            .clicked()
        {
            app.send(Command::RestartContainer(c.name.clone()));
        }
        if paused {
            if ui.button("▷").on_hover_text("Reprendre").clicked() {
                app.send(Command::UnpauseContainer(c.name.clone()));
            }
        } else if ui
            .add_enabled(running, egui::Button::new("⏸"))
            .on_hover_text("Pause")
            .clicked()
        {
            app.send(Command::PauseContainer(c.name.clone()));
        }
        if ui
            .button("📜")
            .on_hover_text("Détail + logs live")
            .clicked()
        {
            app.page = Page::ContainerDetail(c.name.clone());
        }
        if ui.button("⌨").on_hover_text("Terminal").clicked() {
            app.exec_target = Some(format!("docker:{}", c.name));
            app.page = Page::Terminal;
        }
        if ui.button("🗑").on_hover_text("Supprimer").clicked() {
            app.send(Command::RemoveContainer(c.name.clone()));
        }
    });

    ui.end_row();
}
