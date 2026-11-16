//! Vue détail d'un conteneur : actions, stats live, logs en direct,
//! JSON d'inspection.

use eframe::egui;
use naveo_core::Command;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui, name: &str) {
    let p = app.palette;
    let name = name.to_string();

    let Some(c) = app.containers.iter().find(|c| c.name == name).cloned() else {
        page_header(ui, &p, &name, "conteneur introuvable");
        ui.label(
            egui::RichText::new("Le conteneur a peut-être été supprimé. ↻ pour rafraîchir.")
                .color(p.dim),
        );
        return;
    };

    // En-tête : nom, statut, actions.
    page_header(ui, &p, &c.name, &format!("{} · {}", c.image, c.status));

    let running = c.state == "running";
    let paused = c.state == "paused";
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!running, egui::Button::new("▶ Démarrer"))
            .clicked()
        {
            app.send(Command::StartContainer(c.name.clone()));
        }
        if ui
            .add_enabled(running, egui::Button::new("⏹ Arrêter"))
            .clicked()
        {
            app.send(Command::StopContainer(c.name.clone()));
        }
        if ui
            .add_enabled(running, egui::Button::new("🔄 Redémarrer"))
            .clicked()
        {
            app.send(Command::RestartContainer(c.name.clone()));
        }
        if ui
            .add_enabled(running && !paused, egui::Button::new("⏸ Pause"))
            .clicked()
        {
            app.send(Command::PauseContainer(c.name.clone()));
        }
        if ui
            .add_enabled(paused, egui::Button::new("▷ Reprendre"))
            .clicked()
        {
            app.send(Command::UnpauseContainer(c.name.clone()));
        }
        if ui
            .add_enabled(running, egui::Button::new("⚡ Kill"))
            .clicked()
        {
            app.send(Command::KillContainer(c.name.clone()));
        }
        if ui.button("⌨ Terminal").clicked() {
            app.exec_target = Some(format!("docker:{}", c.name));
            app.page = crate::app::Page::Terminal;
        }
        if ui.button("🗑 Supprimer").clicked() {
            app.send(Command::RemoveContainer(c.name.clone()));
            app.page = crate::app::Page::Conteneurs;
        }
    });

    ui.add_space(8.0);

    // Info réseau + projet.
    theme::card(ui, &p, |ui| {
        egui::Grid::new("detail-info")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label("État");
                let color = theme::state_color(&p, &c.state);
                ui.horizontal(|ui| {
                    ui.colored_label(color, "●");
                    ui.label(&c.state);
                });
                ui.end_row();

                ui.label("Ports");
                ui.label(if c.ports.is_empty() {
                    "—".to_string()
                } else {
                    c.ports.join(", ")
                });
                ui.end_row();

                ui.label("Projet Compose");
                ui.label(c.project.clone().unwrap_or_else(|| "—".into()));
                ui.end_row();

                ui.label("ID");
                ui.label(egui::RichText::new(&c.id).color(p.dim).monospace());
                ui.end_row();
            });
    });

    ui.add_space(8.0);

    // Stats live.
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("Ressources (live)").strong());
        ui.add_space(4.0);

        match app.stats.get(&c.name).copied() {
            Some(s) => {
                egui::Grid::new("detail-stats")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("CPU");
                        ui.add(
                            egui::ProgressBar::new(
                                ((s.cpu_percent / 100.0) as f32).clamp(0.0, 1.0),
                            )
                            .show_percentage()
                            .desired_width(180.0),
                        );
                        ui.end_row();

                        ui.label("Mémoire");
                        ui.add(
                            egui::ProgressBar::new(
                                ((s.mem_percent / 100.0) as f32).clamp(0.0, 1.0),
                            )
                            .show_percentage()
                            .desired_width(180.0),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                theme::human_size(s.mem_used_bytes),
                                theme::human_size(s.mem_limit_bytes)
                            ))
                            .size(12.0)
                            .color(p.dim),
                        );
                        ui.end_row();

                        ui.label("Réseau");
                        ui.label(format!(
                            "↓ {} · ↑ {}",
                            theme::human_size(s.net_rx_bytes),
                            theme::human_size(s.net_tx_bytes)
                        ));
                        ui.end_row();
                    });
            }
            None => {
                ui.label(egui::RichText::new("Collecte des stats…").color(p.dim));
            }
        }
    });

    ui.add_space(8.0);

    // Logs en direct avec filtre.
    let log_key = format!("docker:{}", c.name);
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Logs").strong());
            let is_following = app.following.contains(&log_key);
            if ui
                .button(if is_following {
                    "⏹ Stop"
                } else {
                    "⇉ Suivre"
                })
                .clicked()
            {
                if is_following {
                    app.following.remove(&log_key);
                    app.send(Command::StopFollow {
                        key: log_key.clone(),
                    });
                } else {
                    app.following.insert(log_key.clone());
                    app.send(Command::FollowLogs {
                        key: log_key.clone(),
                    });
                }
            }
            if ui
                .button("🧹")
                .on_hover_text("Effacer l'affichage")
                .clicked()
            {
                app.logs.remove(&log_key);
            }
            ui.add_sized(
                [220.0, 22.0],
                egui::TextEdit::singleline(&mut app.log_filter).hint_text("🔍 filtrer…"),
            );
        });

        let mut text = app.logs.get(&log_key).cloned().unwrap_or_default();
        let filter = app.log_filter.to_lowercase();
        if !filter.is_empty() {
            text = text
                .lines()
                .filter(|l| l.to_lowercase().contains(&filter))
                .collect::<Vec<_>>()
                .join("\n");
        }

        egui::ScrollArea::vertical()
            .max_height(320.0)
            .auto_shrink([false, false])
            .stick_to_bottom(!is_following_disabled(app, &log_key))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut text.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(14)
                        .desired_width(ui.available_width())
                        .interactive(true),
                );
            });
    });

    ui.add_space(8.0);

    // Inspect JSON.
    theme::card(ui, &p, |ui| {
        let inspect_key = format!("docker:{}", c.name);
        egui::CollapsingHeader::new(egui::RichText::new("Inspect (JSON)").strong())
            .default_open(false)
            .show(ui, |ui| match app.inspect.get(&inspect_key) {
                Some(json) => {
                    let json = json.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut json.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_rows(18)
                            .desired_width(ui.available_width())
                            .interactive(true),
                    );
                }
                None => {
                    ui.label(egui::RichText::new("Chargement…").color(p.dim));
                }
            });
    });
}

fn is_following_disabled(_app: &NaveoApp, _key: &str) -> bool {
    // stick_to_bottom tant qu'on suit ; simplifié : toujours collé en bas.
    false
}
