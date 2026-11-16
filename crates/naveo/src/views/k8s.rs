//! Vues Kubernetes — Nœuds, Pods (drill-down par conteneur), Services,
//! Déploiements, avec filtre de namespace.

use eframe::egui;
use naveo_core::Command;

use crate::app::NaveoApp;
use crate::theme;
use crate::views::page_header;

/// Sélecteur de namespace commun aux pages K8s.
fn ns_selector(app: &mut NaveoApp, ui: &mut egui::Ui, all: Vec<String>) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Namespace :").size(12.0).color(p.dim));
        egui::ComboBox::from_id_salt("ns-filter")
            .selected_text(app.ns_filter.clone().unwrap_or_else(|| "Tous".to_string()))
            .width(180.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.ns_filter, None, "Tous");
                for ns in all {
                    ui.selectable_value(&mut app.ns_filter, Some(ns.clone()), &ns);
                }
            });
    });
}

fn ns_matches(app: &NaveoApp, ns: &str) -> bool {
    app.ns_filter.as_deref().is_none_or(|f| f == ns)
}

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
            "Kubernetes n'est pas connecté. Choisis un cluster dans la barre du haut.",
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
// Pods (avec drill-down par conteneur)
// ---------------------------------------------------------------------------

pub fn pods(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let pods: Vec<_> = app
        .pods
        .clone()
        .into_iter()
        .filter(|pod| {
            ns_matches(app, &pod.namespace)
                && (search.is_empty()
                    || pod.name.to_lowercase().contains(&search)
                    || pod.namespace.to_lowercase().contains(&search))
        })
        .collect();

    let namespaces: Vec<String> = {
        let mut v: Vec<String> = app.pods.iter().map(|p| p.namespace.clone()).collect();
        v.sort();
        v.dedup();
        v
    };

    page_header(ui, &p, "Pods", &format!("{} pod(s)", pods.len()));

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    ns_selector(app, ui, namespaces);
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            for pod in &pods {
                let ready = pod.containers.iter().filter(|c| c.ready).count();
                let total = pod.containers.len();
                let id = format!("pod-{}-{}", pod.namespace, pod.name);

                let header = format!(
                    "● {} / {}   {}  ·  {}/{} prêt(s)  ·  {} redémarrage(s)",
                    pod.namespace, pod.name, pod.phase, ready, total, pod.restarts
                );

                egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                    .id_salt(&id)
                    .default_open(false)
                    .show(ui, |ui| {
                        // Métadonnées du pod.
                        egui::Grid::new(format!("{id}-meta"))
                            .num_columns(2)
                            .spacing([14.0, 3.0])
                            .show(ui, |ui| {
                                ui.label("Nœud");
                                ui.label(egui::RichText::new(&pod.node).color(p.dim));
                                ui.end_row();
                                ui.label("IP");
                                ui.label(egui::RichText::new(&pod.pod_ip).color(p.dim));
                                ui.end_row();
                                if !pod.owner.is_empty() {
                                    ui.label("Propriétaire");
                                    ui.label(egui::RichText::new(&pod.owner).color(p.accent));
                                    ui.end_row();
                                }
                            });
                        ui.add_space(4.0);

                        // Un par conteneur : état, raison, redémarrages.
                        ui.label(egui::RichText::new("Conteneurs").size(12.0).color(p.dim));
                        egui::Grid::new(format!("{id}-containers"))
                            .num_columns(6)
                            .spacing([14.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for c in &pod.containers {
                                    let ccolor = theme::state_color(&p, &c.state);
                                    ui.horizontal(|ui| {
                                        ui.colored_label(ccolor, "●");
                                        ui.label(egui::RichText::new(&c.name).strong());
                                    });
                                    ui.label(egui::RichText::new(&c.image).size(11.0).color(p.dim));
                                    ui.label(if c.ready { "prêt" } else { "pas prêt" });
                                    ui.label(&c.state);
                                    ui.label(if c.reason.is_empty() {
                                        "—".to_string()
                                    } else {
                                        c.reason.clone()
                                    });
                                    ui.label(format!("{} redém.", c.restarts));
                                    ui.end_row();
                                }
                            });

                        // Actions pod.
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let follow_key = format!("k8s:{}/{}", pod.namespace, pod.name);
                            let following = app.following.contains(&follow_key);
                            if ui
                                .button(if following {
                                    "⏹ Stop logs"
                                } else {
                                    "⇉ Suivre logs"
                                })
                                .clicked()
                            {
                                if following {
                                    app.following.remove(&follow_key);
                                    app.send(Command::StopFollow { key: follow_key });
                                } else {
                                    app.following.insert(follow_key.clone());
                                    app.send(Command::FollowLogs { key: follow_key });
                                    app.page = crate::app::Page::Journal;
                                }
                            }
                            if ui
                                .button("📜")
                                .on_hover_text("Logs (500 dernières lignes)")
                                .clicked()
                            {
                                app.send(Command::PodLogs {
                                    namespace: pod.namespace.clone(),
                                    name: pod.name.clone(),
                                    container: pod
                                        .containers
                                        .first()
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default(),
                                    previous: false,
                                });
                                app.page = crate::app::Page::Journal;
                            }
                            if ui
                                .button("↺")
                                .on_hover_text("Logs précédents (crash)")
                                .clicked()
                            {
                                app.send(Command::PodLogs {
                                    namespace: pod.namespace.clone(),
                                    name: pod.name.clone(),
                                    container: pod
                                        .containers
                                        .first()
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default(),
                                    previous: true,
                                });
                                app.page = crate::app::Page::Journal;
                            }
                            if ui.button("⌨").on_hover_text("Terminal").clicked() {
                                app.exec_target =
                                    Some(format!("k8s:{}/{}", pod.namespace, pod.name));
                                app.page = crate::app::Page::Terminal;
                            }
                            if ui.button("🗑").on_hover_text("Supprimer le pod").clicked() {
                                app.send(Command::DeletePod {
                                    namespace: pod.namespace.clone(),
                                    name: pod.name.clone(),
                                });
                            }
                        });
                    });
                ui.add_space(2.0);
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

pub fn services(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let services: Vec<_> = app
        .services
        .clone()
        .into_iter()
        .filter(|s| {
            ns_matches(app, &s.namespace)
                && (search.is_empty()
                    || s.name.to_lowercase().contains(&search)
                    || s.namespace.to_lowercase().contains(&search))
        })
        .collect();

    let namespaces: Vec<String> = {
        let mut v: Vec<String> = app.services.iter().map(|s| s.namespace.clone()).collect();
        v.sort();
        v.dedup();
        v
    };

    page_header(
        ui,
        &p,
        "Services",
        &format!("{} service(s)", services.len()),
    );

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    // Carte port-forward (création + forwards actifs).
    port_forwards(app, ui);
    ui.add_space(4.0);

    ns_selector(app, ui, namespaces);
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("services-grid")
                .num_columns(5)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Service");
                    ui.strong("Type");
                    ui.strong("Cluster IP");
                    ui.strong("Ports");
                    ui.end_row();

                    for s in &services {
                        ui.label(egui::RichText::new(&s.namespace).color(p.dim));
                        ui.label(egui::RichText::new(&s.name).strong());
                        ui.label(&s.service_type);
                        ui.label(egui::RichText::new(&s.cluster_ip).monospace());
                        ui.label(egui::RichText::new(s.ports.join(", ")).size(12.0));
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
            ns_matches(app, &d.namespace)
                && (search.is_empty()
                    || d.name.to_lowercase().contains(&search)
                    || d.namespace.to_lowercase().contains(&search))
        })
        .collect();

    let namespaces: Vec<String> = {
        let mut v: Vec<String> = app
            .deployments
            .iter()
            .map(|d| d.namespace.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    };

    page_header(
        ui,
        &p,
        "Déploiements",
        &format!("{} déploiement(s)", deps.len()),
    );

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    ns_selector(app, ui, namespaces);
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("deploys-grid")
                .num_columns(5)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Déploiement");
                    ui.strong("Replicas");
                    ui.strong("Image");
                    ui.strong("Actions");
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

                        // Scale et rollout restart.
                        ui.horizontal(|ui| {
                            if ui
                                .button("−")
                                .on_hover_text("Réduire les réplicas")
                                .clicked()
                            {
                                app.send(Command::ScaleDeployment {
                                    namespace: d.namespace.clone(),
                                    name: d.name.clone(),
                                    replicas: (d.replicas_desired - 1).max(0),
                                });
                            }
                            if ui
                                .button("+")
                                .on_hover_text("Augmenter les réplicas")
                                .clicked()
                            {
                                app.send(Command::ScaleDeployment {
                                    namespace: d.namespace.clone(),
                                    name: d.name.clone(),
                                    replicas: d.replicas_desired + 1,
                                });
                            }
                            if ui.button("↺").on_hover_text("Rollout restart").clicked() {
                                app.send(Command::RolloutRestart {
                                    namespace: d.namespace.clone(),
                                    name: d.name.clone(),
                                });
                            }
                        });
                        ui.end_row();
                    }
                });
        });
    });
}

// ---------------------------------------------------------------------------
// Événements
// ---------------------------------------------------------------------------

pub fn events(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let events: Vec<_> = app
        .k8s_events
        .iter()
        .filter(|e| {
            ns_matches(app, &e.namespace)
                && (search.is_empty()
                    || e.involved.to_lowercase().contains(&search)
                    || e.reason.to_lowercase().contains(&search)
                    || e.message.to_lowercase().contains(&search))
        })
        .cloned()
        .collect();

    let warnings = events.iter().filter(|e| e.event_type == "Warning").count();
    page_header(
        ui,
        &p,
        "Événements",
        &format!("{} événement(s) · {} warning(s)", events.len(), warnings),
    );

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            egui::Grid::new("events-grid")
                .num_columns(7)
                .spacing([14.0, 5.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Niveau");
                    ui.strong("Objet");
                    ui.strong("Raison");
                    ui.strong("Message");
                    ui.strong("Compte");
                    ui.strong("Vu");
                    ui.strong("NS");
                    ui.end_row();

                    for e in &events {
                        let is_warn = e.event_type == "Warning";
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                if is_warn { p.err } else { p.dim },
                                if is_warn { "⚠" } else { "·" },
                            );
                            ui.label(
                                egui::RichText::new(if is_warn { "Warning" } else { "Normal" })
                                    .size(11.0),
                            );
                        });
                        ui.label(egui::RichText::new(&e.involved).size(12.0));
                        ui.label(egui::RichText::new(&e.reason).size(12.0).strong());
                        ui.label(egui::RichText::new(&e.message).size(12.0).color(p.dim));
                        ui.label(e.count.to_string());
                        ui.label(egui::RichText::new(&e.last_seen).size(12.0).color(p.dim));
                        ui.label(egui::RichText::new(&e.namespace).size(12.0).color(p.dim));
                        ui.end_row();
                    }
                });
        });
    });
}

// ---------------------------------------------------------------------------
// Ressources génériques (configmaps, secrets, ingresses, pvcs)
// ---------------------------------------------------------------------------

pub fn resources(app: &mut NaveoApp, ui: &mut egui::Ui, kind: &str) {
    let p = app.palette;
    let (title, empty_msg) = match kind {
        "configmaps" => ("ConfigMaps", "Aucune ConfigMap"),
        "secrets" => ("Secrets", "Aucun secret"),
        "ingresses" => ("Ingress", "Aucun Ingress"),
        "pvcs" => ("Volumes persistants (PVC)", "Aucun PVC"),
        _ => (kind, "Aucune ressource"),
    };
    let search = app.search.to_lowercase();

    let items: Vec<_> = app
        .resources
        .get(kind)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| {
            ns_matches(app, &r.namespace)
                && (search.is_empty()
                    || r.name.to_lowercase().contains(&search)
                    || r.summary.to_lowercase().contains(&search))
        })
        .collect();

    page_header(ui, &p, title, &format!("{} ressource(s)", items.len()));

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            if items.is_empty() {
                ui.label(egui::RichText::new(empty_msg).color(p.dim));
            } else {
                egui::Grid::new(format!("res-{kind}-grid"))
                    .num_columns(4)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Namespace");
                        ui.strong("Nom");
                        ui.strong("Détail");
                        ui.strong("Âge");
                        ui.end_row();

                        for r in &items {
                            ui.label(egui::RichText::new(&r.namespace).color(p.dim));
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&r.name).strong());
                                crate::views::copy_btn(ui, &r.name);
                            });
                            ui.label(egui::RichText::new(&r.summary).size(12.0));
                            ui.label(egui::RichText::new(&r.age).size(12.0).color(p.dim));
                            ui.end_row();
                        }
                    });
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Port-forwards
// ---------------------------------------------------------------------------

/// Carte de port-forward, affichée en haut de la page Services.
pub fn port_forwards(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;

    // Formulaire de création.
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("⇄ Port-forward (pod)").strong());
        ui.label(
            egui::RichText::new("Redirige un port local (127.0.0.1) vers un pod du cluster.")
                .size(11.0)
                .color(p.dim),
        );
        ui.horizontal(|ui| {
            ui.label("Namespace");
            ui.add_sized([120.0, 22.0], egui::TextEdit::singleline(&mut app.pf_ns));
            ui.label("Pod");
            ui.add_sized([180.0, 22.0], egui::TextEdit::singleline(&mut app.pf_pod));
            ui.label("Port pod");
            ui.add_sized([70.0, 22.0], egui::TextEdit::singleline(&mut app.pf_target));
            ui.label("→ local");
            ui.add_sized([70.0, 22.0], egui::TextEdit::singleline(&mut app.pf_local));
            if ui.button("⇄ Démarrer").clicked() {
                if let (Ok(target), Ok(local)) = (
                    app.pf_target.trim().parse::<u16>(),
                    app.pf_local.trim().parse::<u16>(),
                ) {
                    if !app.pf_ns.trim().is_empty() && !app.pf_pod.trim().is_empty() {
                        app.send(Command::PortForward {
                            namespace: app.pf_ns.trim().to_string(),
                            pod: app.pf_pod.trim().to_string(),
                            target_port: target,
                            local_port: local,
                        });
                    }
                }
            }
        });
    });

    // Port-forwards actifs.
    if !app.forwards.is_empty() {
        ui.add_space(4.0);
        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("Forwards actifs").strong());
            for f in &app.forwards {
                ui.horizontal(|ui| {
                    ui.colored_label(p.ok, "●");
                    ui.label(
                        egui::RichText::new(format!(
                            "127.0.0.1:{} → {}/{} ({})",
                            f.local_port, f.namespace, f.pod, f.target_port
                        ))
                        .monospace(),
                    );
                    if ui.button("⏹").on_hover_text("Arrêter").clicked() {
                        app.send(Command::StopPortForward { key: f.key.clone() });
                    }
                });
            }
        });
    }
}
