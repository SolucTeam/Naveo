//! Vue Paramètres.

use eframe::egui;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;

    page_header(ui, &p, "Paramètres", "apparence et connexions");

    // Apparence
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("Apparence").strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Thème :");
            if ui
                .selectable_label(app.theme_mode == crate::app::ThemeMode::Sombre, "🌙 Sombre")
                .clicked()
            {
                app.set_theme_mode(crate::app::ThemeMode::Sombre);
            }
            if ui
                .selectable_label(app.theme_mode == crate::app::ThemeMode::Clair, "☀ Clair")
                .clicked()
            {
                app.set_theme_mode(crate::app::ThemeMode::Clair);
            }
            if ui
                .selectable_label(
                    app.theme_mode == crate::app::ThemeMode::Systeme,
                    "🖥 Système",
                )
                .clicked()
            {
                app.set_theme_mode(crate::app::ThemeMode::Systeme);
            }
        });
        ui.label(
            egui::RichText::new("Le choix est mémorisé dans ~/.naveo/prefs.json.")
                .size(12.0)
                .color(p.dim),
        );
    });
    ui.add_space(8.0);

    // Connexions
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("Connexions").strong());
        ui.add_space(4.0);

        let (dot_d, txt_d) = if app.docker_ok {
            (p.ok, "Docker : connecté")
        } else {
            (p.err, "Docker : non connecté")
        };
        ui.horizontal(|ui| {
            ui.colored_label(dot_d, "●");
            ui.label(txt_d);
        });

        let (dot_k, txt_k) = if app.k8s_ok {
            (p.ok, format!("Kubernetes : connecté ({})", app.k8s_ctx))
        } else {
            (p.err, "Kubernetes : non connecté".to_string())
        };
        ui.horizontal(|ui| {
            ui.colored_label(dot_k, "●");
            ui.label(txt_k);
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Docker : socket local (~/Library/Containers/com.docker.docker ou /var/run/docker.sock).\n\
                 Kubernetes : choisis un cluster ci-dessous, ou importe un kubeconfig.",
            )
            .size(12.0)
            .color(p.dim),
        );
    });
    ui.add_space(8.0);

    // Clusters Kubernetes (multi-cluster)
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Clusters Kubernetes").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("📁 Importer un kubeconfig…").clicked() {
                    app.import_kubeconfig();
                }
            });
        });
        ui.add_space(4.0);

        if app.contexts.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Aucun contexte trouvé. Importe un kubeconfig avec le bouton ci-dessus.",
                )
                .color(p.dim),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    let mut picked: Option<(String, String)> = None;
                    for ctx in &app.contexts {
                        ui.horizontal(|ui| {
                            let active = app.k8s_ok && app.k8s_ctx == ctx.name;
                            ui.colored_label(if active { p.ok } else { p.dim }, "●");
                            ui.label(egui::RichText::new(&ctx.name).strong());
                            ui.label(
                                egui::RichText::new(format!("· {}", ctx.cluster)).color(p.dim),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !active,
                                            egui::Button::new(if active {
                                                "Connecté"
                                            } else {
                                                "Se connecter"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        picked = Some((ctx.file.clone(), ctx.name.clone()));
                                    }
                                },
                            );
                        });
                        ui.separator();
                    }
                    if let Some((file, name)) = picked {
                        app.send(naveo_core::Command::ConnectContext {
                            file,
                            context: name,
                        });
                    }
                });
        }
    });
    ui.add_space(8.0);

    // À propos
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("À propos").strong());
        ui.add_space(4.0);
        ui.label("Naveo v0.1.0 — conteneurs + Kubernetes, une seule app native en Rust.");
    });
}
