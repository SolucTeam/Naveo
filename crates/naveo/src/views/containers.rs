//! Vue Conteneurs — groupés par projet Compose, avec multi-sélection,
//! actions en masse, actions projet et actions par conteneur.

use std::collections::BTreeMap;

use eframe::egui;
use naveo_core::{Command, ContainerInfo};

use crate::app::{NaveoApp, Page};
use crate::theme;
use crate::views::{copy_btn, page_header, sort_header};

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let mut containers: Vec<ContainerInfo> = app
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

    // Tri par colonne : 0 nom, 1 image, 2 état.
    let (col, asc) = app.sort_containers;
    containers.sort_by(|a, b| {
        let ord = match col {
            1 => a.image.cmp(&b.image),
            2 => a.state.cmp(&b.state),
            _ => a.name.cmp(&b.name),
        };
        if asc {
            ord
        } else {
            ord.reverse()
        }
    });

    let running = containers.iter().filter(|c| c.state == "running").count();
    page_header(
        ui,
        &p,
        "Conteneurs",
        &format!(
            "{} conteneur(s) · {} actif(s) · {} sélectionné(s)",
            containers.len(),
            running,
            app.sel_containers.len()
        ),
    );

    if !app.docker_ok {
        ui.colored_label(
            p.warn,
            "Docker n'est pas joignable. Lance un daemon Docker puis ↻.",
        );
        return;
    }

    // Barre d'actions en masse.
    if !app.sel_containers.is_empty() {
        theme::card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} sélectionné(s) :", app.sel_containers.len()))
                        .strong(),
                );
                if ui.button("▶ Démarrer").clicked() {
                    app.send(Command::BulkContainers {
                        names: app.sel_containers.iter().cloned().collect(),
                        action: "start".into(),
                    });
                }
                if ui.button("⏹ Arrêter").clicked() {
                    app.send(Command::BulkContainers {
                        names: app.sel_containers.iter().cloned().collect(),
                        action: "stop".into(),
                    });
                }
                if ui.button("🔄 Redémarrer").clicked() {
                    app.send(Command::BulkContainers {
                        names: app.sel_containers.iter().cloned().collect(),
                        action: "restart".into(),
                    });
                }
                if ui.button("🗑 Supprimer").clicked() {
                    app.send(Command::BulkContainers {
                        names: app.sel_containers.iter().cloned().collect(),
                        action: "remove".into(),
                    });
                }
                if ui.button("✗ Désélectionner").clicked() {
                    app.sel_containers.clear();
                }
            });
        });
        ui.add_space(4.0);
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

                        // En-têtes triables.
                        ui.horizontal(|ui| {
                            ui.label("　"); // colonne des cases
                            sort_header(ui, &mut app.sort_containers, 0, "Nom");
                            sort_header(ui, &mut app.sort_containers, 1, "Image");
                            sort_header(ui, &mut app.sort_containers, 2, "État");
                        });
                        ui.separator();

                        egui::Grid::new(format!("grid-{group}"))
                            .num_columns(7)
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
    let id_short = c.id.chars().take(12).collect::<String>();

    // Case de sélection multiple.
    let mut selected = app.sel_containers.contains(&c.name);
    if ui.checkbox(&mut selected, "").changed() {
        if selected {
            app.sel_containers.insert(c.name.clone());
        } else {
            app.sel_containers.remove(&c.name);
        }
    }

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

    // ID + copie
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&id_short)
                .color(p.dim)
                .monospace()
                .size(11.0),
        );
        copy_btn(ui, &c.id);
    });

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
