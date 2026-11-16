//! Vues Kubernetes — Nœuds, Pods, Déploiements (inspirées de Headlamp).

use eframe::egui;
use naveo_core::Command;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

// ---------------------------------------------------------------------------
// Nœuds
// ---------------------------------------------------------------------------

pub fn nodes(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let nodes: Vec<_> = app
        .nodes
        .clone()
        .into_iter()
        .filter(|n| search.is_empty() || n.name.to_lowercase().contains(&search))
        .collect();

    page_header(ui, &p, "Nœuds", &format!("{} nœud(s)", nodes.len()));

    if !app.k8s_ok {
        ui.colored_label(
            p.warn,
            "Kubernetes n'est pas joignable. Vérifie ton kubeconfig (~/.kube/config).",
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for n in &nodes {
            theme::card(ui, &p, |ui| {
                egui::Grid::new(format!("node-{}", n.name))
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        let dot = if n.ready { p.ok } else { p.err };
                        let _etat = if n.ready { "Prêt" } else { "Pas prêt" };

                        ui.horizontal(|ui| {
                            ui.colored_label(dot, "●");
                            ui.label(egui::RichText::new(&n.name).strong().size(16.0));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !n.roles.is_empty() {
                                ui.label(egui::RichText::new(&n.roles).color(p.accent).size(12.0));
                            }
                        });
                        ui.end_row();

                        ui.label("Kubelet");
                        ui.label(egui::RichText::new(&n.kubelet_version).color(p.dim));
                        ui.end_row();

                        ui.label("IP");
                        ui.label(&n.internal_ip);
                        ui.end_row();

                        ui.label("Allocatable");
                        ui.label(format!("CPU {} · mémoire {}", n.cpu, n.memory));
                        ui.end_row();
                    });
            });
            ui.add_space(4.0);
        }
    });
}

// ---------------------------------------------------------------------------
// Pods
// ---------------------------------------------------------------------------

pub fn pods(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let pods: Vec<_> = app
        .pods
        .clone()
        .into_iter()
        .filter(|pod| {
            search.is_empty()
                || pod.name.to_lowercase().contains(&search)
                || pod.namespace.to_lowercase().contains(&search)
        })
        .collect();

    page_header(ui, &p, "Pods", &format!("{} pod(s)", pods.len()));

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas joignable.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("pods-grid")
                .num_columns(7)
                .spacing([14.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Pod");
                    ui.strong("Phase");
                    ui.strong("Redémarrages");
                    ui.strong("Conteneurs");
                    ui.strong("");
                    ui.strong("");
                    ui.end_row();

                    for pod in &pods {
                        ui.label(egui::RichText::new(&pod.namespace).color(p.dim));
                        ui.label(egui::RichText::new(&pod.name).strong());

                        let color = theme::state_color(&p, &pod.phase);
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            ui.label(&pod.phase);
                        });

                        ui.label(if pod.restarts > 0 {
                            egui::RichText::new(format!("{}", pod.restarts))
                                .color(if pod.restarts > 5 { p.err } else { p.text })
                        } else {
                            egui::RichText::new("0").color(p.dim)
                        });

                        ui.label(
                            egui::RichText::new(pod.containers.join(", ")).size(12.0).color(p.dim),
                        );

                        if ui.button("📜").on_hover_text("Logs").clicked() {
                            app.send(Command::PodLogs {
                                namespace: pod.namespace.clone(),
                                name: pod.name.clone(),
                                container: pod.containers.first().cloned().unwrap_or_default(),
                            });
                        }
                        if ui.button("🗑").on_hover_text("Supprimer le pod").clicked() {
                            app.send(Command::DeletePod {
                                namespace: pod.namespace.clone(),
                                name: pod.name.clone(),
                            });
                        }
                        ui.end_row();
                    }
                });
        });
    });
}

// ---------------------------------------------------------------------------
// Déploiements
// ---------------------------------------------------------------------------

pub fn deployments(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let deps: Vec<_> = app
        .deployments
        .clone()
        .into_iter()
        .filter(|d| {
            search.is_empty()
                || d.name.to_lowercase().contains(&search)
                || d.namespace.to_lowercase().contains(&search)
        })
        .collect();

    page_header(
        ui,
        &p,
        "Déploiements",
        &format!("{} déploiement(s)", deps.len()),
    );

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas joignable.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("deploys-grid")
                .num_columns(4)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Déploiement");
                    ui.strong("Replicas");
                    ui.strong("Image");
                    ui.end_row();

                    for d in &deps {
                        ui.label(egui::RichText::new(&d.namespace).color(p.dim));
                        ui.label(egui::RichText::new(&d.name).strong());

                        let ok = d.replicas_ready == d.replicas_desired;
                        ui.horizontal(|ui| {
                            ui.colored_label(if ok { p.ok } else { p.warn }, "●");
                            ui.label(format!("{}/{}", d.replicas_ready, d.replicas_desired));
                        });

                        ui.label(egui::RichText::new(&d.image).size(12.0).color(p.dim));
                        ui.end_row();
                    }
                });
        });
    });
}
