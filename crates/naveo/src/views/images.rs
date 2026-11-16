//! Vue Images — pull avec recherche Docker Hub, exécution avec env/ports,
//! tag / save / load / historique, registries, prune, multi-sélection.

use eframe::egui;
use naveo_core::Command;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::{copy_btn, page_header, sort_header};

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let mut images: Vec<_> = app
        .images
        .clone()
        .into_iter()
        .filter(|i| search.is_empty() || i.tags.iter().any(|t| t.to_lowercase().contains(&search)))
        .collect();

    // Tri : 0 tag, 1 taille, 2 date.
    let (col, asc) = app.sort_images;
    images.sort_by(|a, b| {
        let a_tag = a.tags.first().cloned().unwrap_or_default();
        let b_tag = b.tags.first().cloned().unwrap_or_default();
        let ord = match col {
            1 => a.size_bytes.cmp(&b.size_bytes),
            2 => a.created.cmp(&b.created),
            _ => a_tag.cmp(&b_tag),
        };
        if asc {
            ord
        } else {
            ord.reverse()
        }
    });

    let total_bytes: u64 = images.iter().map(|i| i.size_bytes).sum();

    page_header(
        ui,
        &p,
        "Images",
        &format!(
            "{} image(s) · {} · {} sélectionnée(s)",
            images.len(),
            theme::human_size(total_bytes),
            app.sel_images.len()
        ),
    );

    if !app.docker_ok {
        ui.colored_label(p.warn, "Docker n'est pas joignable.");
        return;
    }

    // Barre d'outils : pull + recherche Hub + registres + prune.
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⬇ Pull :").strong());
            let resp = ui.add_sized(
                [280.0, 24.0],
                egui::TextEdit::singleline(&mut app.pull_field)
                    .hint_text("nginx:alpine, user/image…"),
            );
            let pull_now = ui.button("Tirer").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if pull_now && !app.pull_field.trim().is_empty() {
                let image = app.pull_field.trim().to_string();
                app.pull_field.clear();
                app.hub_query = image.clone();
                app.send(Command::HubSearch {
                    query: image.split(':').next().unwrap_or(&image).to_string(),
                });
                app.send(Command::PullImage { image });
            }

            ui.separator();
            ui.label(egui::RichText::new("🔍 Docker Hub :").strong());
            let search_resp = ui.add_sized(
                [220.0, 24.0],
                egui::TextEdit::singleline(&mut app.hub_query).hint_text("rechercher une image…"),
            );
            if ui.button("Chercher").clicked()
                || (search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                let q = app.hub_query.trim().to_string();
                if !q.is_empty() {
                    app.send(Command::HubSearch { query: q });
                }
            }
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui.button("⬆ Importer (tar)…").clicked() {
                app.load_open = true;
            }
            if ui.button("📦 Registries…").clicked() {
                app.registry_open = !app.registry_open;
            }
            if ui.button("🧹 Nettoyer les images").clicked() {
                app.prune_confirm = Some("images".into());
            }
            if !app.sel_images.is_empty() {
                if ui
                    .button(format!("🗑 Supprimer {} image(s)", app.sel_images.len()))
                    .clicked()
                {
                    for id in app.sel_images.clone() {
                        app.send(Command::RemoveImage(id));
                    }
                    app.sel_images.clear();
                }
                if ui.button("✗ Désélectionner").clicked() {
                    app.sel_images.clear();
                }
            }
        });
    });
    ui.add_space(4.0);

    // Résultats de recherche Docker Hub.
    if !app.hub_results.is_empty() {
        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("Résultats Docker Hub").strong());
            ui.add_space(2.0);
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    egui::Grid::new("hub-grid")
                        .num_columns(5)
                        .spacing([14.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for r in &app.hub_results {
                                ui.horizontal(|ui| {
                                    if r.official {
                                        ui.label("🏛");
                                    }
                                    ui.label(egui::RichText::new(&r.name).strong());
                                    copy_btn(ui, &r.name);
                                });
                                ui.label(
                                    egui::RichText::new(r.description.lines().next().unwrap_or(""))
                                        .size(11.0)
                                        .color(p.dim),
                                );
                                ui.label(egui::RichText::new(format!("⭐ {}", r.stars)).size(11.0));
                                ui.label(
                                    egui::RichText::new(format!("⬇ {}", fmt_count(r.pulls)))
                                        .size(11.0),
                                );
                                if ui.button("⬇ Tirer").clicked() {
                                    app.pull_field = r.name.clone();
                                    app.send(Command::PullImage {
                                        image: r.name.clone(),
                                    });
                                }
                                ui.end_row();
                            }
                        });
                });
            if ui.button("✗ Masquer").clicked() {
                app.hub_results.clear();
            }
        });
        ui.add_space(4.0);
    }

    // Tags d'un dépôt ouvert.
    if let Some(repo) = app.hub_open_repo.clone() {
        if let Some(tags) = app.hub_tags.get(&repo).cloned() {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new(format!("Tags de {repo}")).strong());
                ui.horizontal_wrapped(|ui| {
                    for t in &tags {
                        let full = format!("{repo}:{t}");
                        if ui.button(full.clone()).clicked() {
                            app.send(Command::PullImage { image: full });
                        }
                    }
                });
                if ui.button("✗ Fermer").clicked() {
                    app.hub_open_repo = None;
                }
            });
        }
    }

    // Registries (login / logout).
    if app.registry_open {
        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("Registres").strong());
            egui::Grid::new("reg-grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Serveur");
                    ui.add_sized(
                        [240.0, 22.0],
                        egui::TextEdit::singleline(&mut app.reg_server)
                            .hint_text("registry.example.com"),
                    );
                    ui.end_row();
                    ui.label("Utilisateur");
                    ui.add_sized([240.0, 22.0], egui::TextEdit::singleline(&mut app.reg_user));
                    ui.end_row();
                    ui.label("Mot de passe / token");
                    ui.add_sized(
                        [240.0, 22.0],
                        egui::TextEdit::singleline(&mut app.reg_pass).password(true),
                    );
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                if ui.button("✔ Se connecter").clicked() {
                    app.send(Command::RegistryLogin {
                        server: app.reg_server.trim().to_string(),
                        username: app.reg_user.trim().to_string(),
                        password: app.reg_pass.clone(),
                    });
                    app.reg_pass.clear();
                    app.registry_open = false;
                }
                if ui.button("✗ Se déconnecter").clicked() {
                    app.send(Command::RegistryLogout(app.reg_server.trim().to_string()));
                }
                if ui.button("Fermer").clicked() {
                    app.registry_open = false;
                }
            });
        });
        ui.add_space(4.0);
    }

    // Dialogue « exécuter un conteneur ».
    if app.run_open {
        run_dialog(app, ui);
        ui.add_space(4.0);
    }

    // Dialogues tag / import.
    dialogs(app, ui);

    // Confirmation de prune.
    if let Some(kind) = app.prune_confirm.clone() {
        egui::Area::new(egui::Id::new("prune-confirm"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                theme::card(ui, &p, |ui| {
                    ui.label(egui::RichText::new("Confirmer le nettoyage").strong());
                    ui.label(
                        "Supprime toutes les images inutilisées (non taguées et non référencées).",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("✔ Nettoyer").clicked() {
                            app.send(Command::Prune { kind: kind.clone() });
                            app.prune_confirm = None;
                        }
                        if ui.button("Annuler").clicked() {
                            app.prune_confirm = None;
                        }
                    });
                });
            });
    }

    // Liste des images.
    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                sort_header(ui, &mut app.sort_images, 0, "Image");
                sort_header(ui, &mut app.sort_images, 1, "Taille");
                sort_header(ui, &mut app.sort_images, 2, "Créée");
            });
            ui.separator();
            egui::Grid::new("images-grid")
                .num_columns(6)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for img in &images {
                        let tag = img
                            .tags
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "<sans tag>".into());
                        let extra = img.tags.len().saturating_sub(1);

                        // Sélection multiple.
                        let mut selected = app.sel_images.contains(&tag);
                        if ui.checkbox(&mut selected, "").changed() {
                            if selected {
                                app.sel_images.insert(tag.clone());
                            } else {
                                app.sel_images.remove(&tag);
                            }
                        }

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&tag).strong());
                            if extra > 0 {
                                ui.label(
                                    egui::RichText::new(format!("+{extra}"))
                                        .size(11.0)
                                        .color(p.dim),
                                );
                            }
                            copy_btn(ui, &img.id);
                        });
                        ui.label(theme::human_size(img.size_bytes));
                        ui.label(
                            egui::RichText::new(format!("il y a {}", days_ago(img.created)))
                                .color(p.dim),
                        );

                        // Actions avancées.
                        ui.horizontal(|ui| {
                            if ui.button("▶").on_hover_text("Exécuter").clicked() {
                                app.run_open = true;
                                app.run_image = tag.clone();
                                app.run_name = suggested_name(&tag);
                                app.run_env = vec![(String::new(), String::new())];
                                app.run_ports = vec![(String::new(), String::new())];
                            }
                            if ui.button("🏷").on_hover_text("Nouveau tag").clicked() {
                                app.pending_tag = Some(tag.clone());
                                app.tag_field =
                                    format!("{}:v2", tag.split(':').next().unwrap_or(&tag));
                            }
                            if ui.button("⬇").on_hover_text("Exporter (tar)").clicked() {
                                if let Some(dest) = rfd::FileDialog::new()
                                    .set_title("Exporter l'image")
                                    .set_file_name(format!("{}.tar", tag.replace('/', "_")))
                                    .save_file()
                                {
                                    app.send(Command::SaveImage {
                                        image: tag.clone(),
                                        path: dest.display().to_string(),
                                    });
                                }
                            }
                            if ui
                                .button("🧬")
                                .on_hover_text("Historique des couches")
                                .clicked()
                            {
                                app.send(Command::ImageHistory(tag.clone()));
                                app.history_open = Some(
                                    app.history_open
                                        .take()
                                        .filter(|h| h != &tag)
                                        .unwrap_or_else(|| tag.clone()),
                                );
                            }
                            if ui.button("🗑").on_hover_text("Supprimer").clicked() {
                                let target = if tag.contains('@') || tag == "<sans tag>" {
                                    img.id.clone()
                                } else {
                                    tag.clone()
                                };
                                app.send(Command::RemoveImage(target));
                            }
                        });
                        ui.end_row();

                        // Historique des couches (extensible).
                        if app.history_open.as_deref() == Some(tag.as_str()) {
                            history_block(app, ui, &tag);
                        }
                    }
                });
        });
    });
}

/// Bloc « historique des couches » d'une image.
fn history_block(app: &mut NaveoApp, ui: &mut egui::Ui, tag: &str) {
    let p = app.palette;
    ui.label("");
    ui.end_row();
    ui.label("");
    ui.with_layout(
        egui::Layout::top_down_justified(egui::Align::LEFT),
        |ui| match app.image_history.get(tag) {
            Some(layers) => {
                for l in layers {
                    let cmd = l.created_by.lines().next().unwrap_or("").to_string();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            theme::human_size(l.size_bytes),
                            cmd
                        ))
                        .size(11.0)
                        .monospace()
                        .color(p.dim),
                    );
                }
            }
            None => {
                ui.label(egui::RichText::new("Chargement des couches…").color(p.dim));
            }
        },
    );
    ui.end_row();
}

/// Dialogue d'exécution : nom, variables d'env, ports.
fn run_dialog(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new(format!("Exécuter « {} »", app.run_image)).strong());
        egui::Grid::new("run-grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("Nom du conteneur");
                ui.add_sized([300.0, 22.0], egui::TextEdit::singleline(&mut app.run_name));
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Variables d'environnement")
                .size(12.0)
                .color(p.dim),
        );
        let mut env_del: Option<usize> = None;
        for (i, (k, v)) in app.run_env.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [160.0, 22.0],
                    egui::TextEdit::singleline(k).hint_text("CLÉ"),
                );
                ui.add_sized(
                    [220.0, 22.0],
                    egui::TextEdit::singleline(v).hint_text("valeur"),
                );
                if ui.button("✗").clicked() {
                    env_del = Some(i);
                }
            });
        }
        if let Some(i) = env_del {
            app.run_env.remove(i);
        }
        if ui.button("+ Ajouter une variable").clicked() {
            app.run_env.push((String::new(), String::new()));
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Ports (conteneur → hôte)")
                .size(12.0)
                .color(p.dim),
        );
        let mut port_del: Option<usize> = None;
        for (i, (c, h)) in app.run_ports.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [100.0, 22.0],
                    egui::TextEdit::singleline(c).hint_text("8080"),
                );
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
            app.run_ports.remove(i);
        }
        if ui.button("+ Ajouter un port").clicked() {
            app.run_ports.push((String::new(), String::new()));
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("✔ Lancer").clicked() {
                app.send(Command::RunContainer {
                    image: app.run_image.clone(),
                    name: app.run_name.clone(),
                    env: app.run_env.clone(),
                    ports: app.run_ports.clone(),
                });
                app.run_open = false;
            }
            if ui.button("Annuler").clicked() {
                app.run_open = false;
            }
        });
    });
}

/// Dialogues de la vue Images : nouveau tag, import d'image.
fn dialogs(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;

    // Dialogue « appliquer un nouveau tag ».
    if let Some(image) = app.pending_tag.clone() {
        egui::Area::new(egui::Id::new("tag-dialog"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                theme::card(ui, &p, |ui| {
                    ui.label(egui::RichText::new("Nouveau tag").strong());
                    ui.label(egui::RichText::new(&image).size(12.0).color(p.dim));
                    ui.add_space(4.0);
                    ui.add_sized(
                        [340.0, 24.0],
                        egui::TextEdit::singleline(&mut app.tag_field)
                            .hint_text("monrepo/monimage:v2"),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("✔ Appliquer").clicked() {
                            let t = app.tag_field.trim().to_string();
                            if !t.is_empty() {
                                app.send(Command::TagImage {
                                    image: image.clone(),
                                    tag: t,
                                });
                            }
                            app.pending_tag = None;
                        }
                        if ui.button("Annuler").clicked() {
                            app.pending_tag = None;
                        }
                    });
                });
            });
    }

    // Import d'image (bouton en barre d'outils géré ici).
    if app.load_open {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Importer une image (tar)")
            .add_filter("Archive tar", &["tar", "tar.gz"])
            .pick_file()
        {
            app.send(Command::LoadImage {
                path: path.display().to_string(),
            });
        }
        app.load_open = false;
    }
}

fn suggested_name(tag: &str) -> String {
    let base = tag
        .split(':')
        .next()
        .unwrap_or(tag)
        .rsplit('/')
        .next()
        .unwrap_or(tag);
    format!("{}-1", base.replace(['.', '_'], "-"))
}

fn fmt_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}G", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
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
