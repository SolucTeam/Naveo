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
        if ui.button("✏ Renommer").clicked() {
            app.rename_field = c.name.clone();
            app.rename_pending = Some(c.name.clone());
        }
        if ui.button("📸 Commit").clicked() {
            app.commit_field = format!("naveo/{}", c.name);
            app.commit_pending = Some(c.name.clone());
        }
        if ui.button("⬇ Export").clicked() {
            if let Some(dest) = rfd::FileDialog::new()
                .set_title("Exporter le conteneur (tar)")
                .set_file_name(format!("{}.tar", c.name))
                .save_file()
            {
                app.send(Command::ExportContainer {
                    name: c.name.clone(),
                    path: dest.display().to_string(),
                });
            }
        }
        if ui.button("🗑 Supprimer").clicked() {
            app.send(Command::RemoveContainer(c.name.clone()));
            app.page = crate::app::Page::Conteneurs;
        }
    });

    // Dialogues rename / commit.
    if let Some(ref target) = app.rename_pending {
        if target == &c.name {
            rename_dialog(app, ui);
        }
    }
    if let Some(ref target) = app.commit_pending {
        if target == &c.name {
            commit_dialog(app, ui);
        }
    }

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

    // Logs en direct avec filtre texte, filtre par niveau, timestamps,
    // compteur de lignes et téléchargement.
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
            // Reload one-shot (tail configurable + timestamps).
            if ui.button("↺").on_hover_text("Recharger (tail)").clicked() {
                app.send(Command::ContainerLogs {
                    name: c.name.clone(),
                    timestamps: app.log_timestamps,
                    tail: app.log_tail,
                });
            }
            ui.checkbox(&mut app.log_timestamps, "timestamps");
            egui::ComboBox::from_id_salt("log-tail")
                .selected_text(tail_label(app.log_tail))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (label, val) in [("100", 100), ("500", 500), ("2000", 2000), ("tout", 0)] {
                        ui.selectable_value(&mut app.log_tail, val, label);
                    }
                });
            egui::ComboBox::from_id_salt("log-level")
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
            ui.add_sized(
                [180.0, 22.0],
                egui::TextEdit::singleline(&mut app.log_filter).hint_text("🔍 filtrer…"),
            );
            if ui
                .button("💾")
                .on_hover_text("Télécharger les logs")
                .clicked()
            {
                if let Some(dest) = rfd::FileDialog::new()
                    .set_title("Sauvegarder les logs")
                    .set_file_name(format!("{}-logs.txt", c.name))
                    .save_file()
                {
                    let text = app.logs.get(&log_key).cloned().unwrap_or_default();
                    let _ = std::fs::write(dest, text);
                }
            }
        });

        let raw = app.logs.get(&log_key).cloned().unwrap_or_default();
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
        text = crate::views::filter_log_level(&text, &app.log_level);
        ui.label(
            egui::RichText::new(format!("{line_count} ligne(s)"))
                .size(11.0)
                .color(p.dim),
        );

        egui::ScrollArea::vertical()
            .max_height(320.0)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
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

    // Explorateur de fichiers du conteneur (docker cp).
    files_block(app, ui, &c.name);

    ui.add_space(8.0);

    // Édition à chaud : variables d'env / ports (recrée le conteneur).
    edit_block(app, ui, &c.name);

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

/// Explorateur : chemin courant, liste, aperçu, sauvegarde.
fn files_block(app: &mut NaveoApp, ui: &mut egui::Ui, name: &str) {
    let p = app.palette;
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Fichiers").strong());
            // Réinitialise l'explorateur si on a changé de conteneur.
            if app.files_name.as_deref() != Some(name) {
                app.files_name = Some(name.to_string());
                app.files_path = "/".into();
                app.files_entries.clear();
                app.file_preview = None;
            }
            let mut go = false;
            let resp = ui.add_sized(
                [ui.available_width() - 110.0, 22.0],
                egui::TextEdit::singleline(&mut app.files_path)
                    .hint_text("/chemin/dans/le/conteneur"),
            );
            go |= ui.button("↺ Lister").clicked();
            go |= resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if go && !app.files_path.is_empty() {
                app.send(Command::ListFiles {
                    name: name.to_string(),
                    path: app.files_path.clone(),
                });
                app.file_preview = None;
            }
        });

        if !app.files_entries.is_empty() {
            ui.add_space(2.0);
            let entries = app.files_entries.clone();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for e in &entries {
                        ui.horizontal(|ui| {
                            let icon = if e.is_dir { "📁" } else { "📄" };
                            let full =
                                format!("{}/{}", app.files_path.trim_end_matches('/'), e.name);
                            if e.is_dir {
                                if ui
                                    .button(format!("{icon} {}", e.name))
                                    .on_hover_text("Ouvrir ce répertoire")
                                    .clicked()
                                {
                                    app.files_path = full.clone();
                                    app.send(Command::ListFiles {
                                        name: name.to_string(),
                                        path: full,
                                    });
                                    app.file_preview = None;
                                }
                            } else {
                                ui.label(format!("{icon} {}", e.name));
                                if ui.button("👁").on_hover_text("Aperçu").clicked() {
                                    app.send(Command::PreviewFile {
                                        name: name.to_string(),
                                        path: full.clone(),
                                    });
                                }
                                if ui.button("💾").on_hover_text("Sauvegarder").clicked() {
                                    if let Some(dest) = rfd::FileDialog::new()
                                        .set_title("Sauvegarder le fichier")
                                        .set_file_name(&e.name)
                                        .save_file()
                                    {
                                        app.send(Command::SaveFile {
                                            name: name.to_string(),
                                            path: full,
                                            dest: dest.display().to_string(),
                                        });
                                    }
                                }
                                ui.label(
                                    egui::RichText::new(theme::human_size(e.size))
                                        .size(11.0)
                                        .color(p.dim),
                                );
                            }
                        });
                    }
                });
        }

        // Aperçu du fichier sélectionné.
        if let Some((path, content)) = &app.file_preview {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("Aperçu : {path}"))
                    .size(12.0)
                    .strong(),
            );
            let content = content.clone();
            ui.add(
                egui::TextEdit::multiline(&mut content.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(8)
                    .desired_width(ui.available_width())
                    .interactive(false),
            );
        }
    });
}

/// Bloc d'édition env/ports : recrée le conteneur avec la nouvelle config.
fn edit_block(app: &mut NaveoApp, ui: &mut egui::Ui, name: &str) {
    let p = app.palette;
    // Initialise les lignes à l'ouverture de la page (une seule fois).
    if app.edit_container.as_deref() != Some(name) {
        app.edit_container = Some(name.to_string());
        let ports: Vec<(String, String)> = app
            .containers
            .iter()
            .find(|c| c.name == name)
            .map(|c| {
                c.ports
                    .iter()
                    .filter_map(|pp| {
                        // « 0.0.0.0:8080->80/tcp » → (80, 8080)
                        let (host, rest) = pp.split_once("->")?;
                        let cport = rest.split('/').next()?.to_string();
                        let hport = host.rsplit(':').next()?.to_string();
                        Some((cport, hport))
                    })
                    .collect()
            })
            .unwrap_or_default();
        app.edit_ports = if ports.is_empty() {
            vec![(String::new(), String::new())]
        } else {
            ports
        };
        app.edit_env = vec![(String::new(), String::new())];
    }

    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("Configuration (recrée le conteneur)").strong());
        ui.label(
            egui::RichText::new(
                "Les variables d'env et les ports publiés ne sont modifiables qu'en recréant le conteneur (image conservée).",
            )
            .size(11.0)
            .color(p.dim),
        );
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new("Variables d'environnement")
                .size(12.0)
                .color(p.dim),
        );
        let mut env_del: Option<usize> = None;
        for (i, (k, v)) in app.edit_env.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [160.0, 22.0],
                    egui::TextEdit::singleline(k).hint_text("CLÉ"),
                );
                ui.add_sized(
                    [240.0, 22.0],
                    egui::TextEdit::singleline(v).hint_text("valeur"),
                );
                if ui.button("✗").clicked() {
                    env_del = Some(i);
                }
            });
        }
        if let Some(i) = env_del {
            app.edit_env.remove(i);
        }
        if ui.button("+ Variable").clicked() {
            app.edit_env.push((String::new(), String::new()));
        }

        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Ports (conteneur → hôte)")
                .size(12.0)
                .color(p.dim),
        );
        let mut port_del: Option<usize> = None;
        for (i, (c, h)) in app.edit_ports.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized([100.0, 22.0], egui::TextEdit::singleline(c).hint_text("80"));
                ui.label("→");
                ui.add_sized(
                    [100.0, 22.0],
                    egui::TextEdit::singleline(h).hint_text("8080"),
                );
                if ui.button("✗").clicked() {
                    port_del = Some(i);
                }
            });
        }
        if let Some(i) = port_del {
            app.edit_ports.remove(i);
        }
        if ui.button("+ Port").clicked() {
            app.edit_ports.push((String::new(), String::new()));
        }

        ui.add_space(4.0);
        if ui.button("♻ Recréer avec cette configuration").clicked() {
            app.send(Command::RecreateContainer {
                name: name.to_string(),
                env: app.edit_env.clone(),
                ports: app.edit_ports.clone(),
            });
            app.page = crate::app::Page::Conteneurs;
        }
    });
}

/// Dialogue de renommage.
fn rename_dialog(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let name = app.rename_pending.clone().unwrap_or_default();
    egui::Area::new(egui::Id::new("rename-dialog"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new("Renommer le conteneur").strong());
                ui.add_sized(
                    [320.0, 24.0],
                    egui::TextEdit::singleline(&mut app.rename_field),
                );
                ui.horizontal(|ui| {
                    if ui.button("✔ Renommer").clicked() {
                        let new = app.rename_field.trim().to_string();
                        if !new.is_empty() && new != name {
                            app.send(Command::RenameContainer {
                                name: name.clone(),
                                new_name: new,
                            });
                        }
                        app.rename_pending = None;
                    }
                    if ui.button("Annuler").clicked() {
                        app.rename_pending = None;
                    }
                });
            });
        });
}

/// Dialogue de commit.
fn commit_dialog(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let name = app.commit_pending.clone().unwrap_or_default();
    egui::Area::new(egui::Id::new("commit-dialog"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new("Créer une image (commit)").strong());
                ui.label(
                    egui::RichText::new(format!("Source : {name}"))
                        .size(12.0)
                        .color(p.dim),
                );
                ui.add_sized(
                    [340.0, 24.0],
                    egui::TextEdit::singleline(&mut app.commit_field)
                        .hint_text("monrepo/monimage:v1"),
                );
                ui.horizontal(|ui| {
                    if ui.button("✔ Commit").clicked() {
                        let full = app.commit_field.trim().to_string();
                        if let Some((repo, tag)) = full.rsplit_once(':') {
                            app.send(Command::CommitContainer {
                                name: name.clone(),
                                repo: repo.to_string(),
                                tag: tag.to_string(),
                            });
                        } else if !full.is_empty() {
                            app.send(Command::CommitContainer {
                                name: name.clone(),
                                repo: full,
                                tag: "latest".into(),
                            });
                        }
                        app.commit_pending = None;
                    }
                    if ui.button("Annuler").clicked() {
                        app.commit_pending = None;
                    }
                });
            });
        });
}

fn tail_label(tail: i64) -> &'static str {
    match tail {
        100 => "100 lignes",
        2000 => "2000 lignes",
        0 => "tout",
        _ => "500 lignes",
    }
}
