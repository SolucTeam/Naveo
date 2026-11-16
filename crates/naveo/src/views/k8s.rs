//! Vues Kubernetes — Nœuds, Pods (drill-down par conteneur), Services,
//! Déploiements, avec filtre de namespace.

use eframe::egui;
use naveo_core::Command;

use crate::app::{NaveoApp, Page};
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

/// Formatage CPU (milliCPU) : « 250m » ou « 1.5 CPU ».
fn fmt_cpu(millis: u64) -> String {
    if millis >= 1000 {
        format!("{:.1} CPU", millis as f64 / 1000.0)
    } else {
        format!("{millis}m")
    }
}

/// Formatage mémoire (octets) en Ki / Mi / Gi.
fn fmt_mem(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KIB * KIB * KIB {
        format!("{:.1} Gi", b / (KIB * KIB * KIB))
    } else if b >= KIB * KIB {
        format!("{:.1} Mi", b / (KIB * KIB))
    } else if b >= KIB {
        format!("{:.1} Ki", b / KIB)
    } else {
        format!("{bytes} o")
    }
}

/// Page correspondant à un type de ressource (pour « Retour » depuis l'éditeur).
fn page_for_kind(kind: &str) -> Page {
    match kind {
        "pods" => Page::Pods,
        "services" => Page::Services,
        "deployments" => Page::Deploiements,
        "statefulsets" => Page::StatefulSets,
        "daemonsets" => Page::DaemonSets,
        "replicasets" => Page::ReplicaSets,
        "jobs" => Page::Jobs,
        "cronjobs" => Page::CronJobs,
        "configmaps" => Page::ConfigMaps,
        "secrets" => Page::Secrets,
        "ingresses" => Page::Ingress,
        "endpoints" => Page::Endpoints,
        "pvcs" => Page::Stockage,
        "pvs" => Page::PVs,
        "storageclasses" => Page::StorageClasses,
        "serviceaccounts" => Page::ServiceAccounts,
        "roles" => Page::Roles,
        "clusterroles" => Page::ClusterRoles,
        "rolebindings" | "clusterrolebindings" => Page::Bindings,
        "namespaces" => Page::Namespaces,
        "nodes" => Page::Noeuds,
        _ => Page::Pods,
    }
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

                        ui.label("Utilisation");
                        if let Some((cpu, mem)) = app.node_metrics.get(&n.name) {
                            ui.label(
                                egui::RichText::new(format!(
                                    "CPU {} · mémoire {}",
                                    fmt_cpu(*cpu),
                                    fmt_mem(*mem)
                                ))
                                .color(p.accent),
                            );
                        } else {
                            ui.label(egui::RichText::new("— (metrics-server ?)").color(p.dim));
                        }
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

    // Barre d'actions en masse sur la sélection de pods.
    if !app.sel_pods.is_empty() {
        let n = app.sel_pods.len();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{n} pod(s) sélectionné(s)"))
                    .color(p.accent)
                    .strong(),
            );
            if ui.button("🗑 Supprimer la sélection").clicked() {
                let sel: Vec<(String, String)> = app
                    .sel_pods
                    .iter()
                    .filter_map(|k| {
                        k.split_once('/')
                            .map(|(ns, name)| (ns.to_string(), name.to_string()))
                    })
                    .collect();
                app.send(Command::DeletePods { pods: sel });
                app.sel_pods.clear();
            }
            if ui.button("✕ Désélectionner").clicked() {
                app.sel_pods.clear();
            }
        });
        ui.add_space(2.0);
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            for pod in &pods {
                let ready = pod.containers.iter().filter(|c| c.ready).count();
                let total = pod.containers.len();
                let id = format!("pod-{}-{}", pod.namespace, pod.name);
                let pod_key = format!("{}/{}", pod.namespace, pod.name);

                let header = format!(
                    "● {} / {}   {}  ·  {}/{} prêt(s)  ·  {} redémarrage(s)",
                    pod.namespace, pod.name, pod.phase, ready, total, pod.restarts
                );

                // Case à cocher + entête dépliable, dans une grille à deux
                // colonnes pour aligner la sélection en masse.
                egui::Grid::new(format!("{id}-row"))
                    .num_columns(2)
                    .spacing([4.0, 0.0])
                    .show(ui, |ui| {
                        let mut sel = app.sel_pods.contains(&pod_key);
                        if ui
                            .checkbox(&mut sel, "")
                            .on_hover_text("Sélectionner pour une action en masse")
                            .changed()
                        {
                            if sel {
                                app.sel_pods.insert(pod_key.clone());
                            } else {
                                app.sel_pods.remove(&pod_key);
                            }
                        }
                        ui.vertical(|ui| {
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
                                                ui.label(
                                                    egui::RichText::new(&pod.owner).color(p.accent),
                                                );
                                                ui.end_row();
                                            }
                                            if let Some((cpu, mem)) = app
                                                .pod_metrics
                                                .get(&format!("{}/{}", pod.namespace, pod.name))
                                            {
                                                ui.label("Utilisation");
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "CPU {} · mémoire {}",
                                                        fmt_cpu(*cpu),
                                                        fmt_mem(*mem)
                                                    ))
                                                    .color(p.accent),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    ui.add_space(4.0);

                                    // Un par conteneur : état, raison, redémarrages.
                                    ui.label(
                                        egui::RichText::new("Conteneurs").size(12.0).color(p.dim),
                                    );
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
                                                ui.label(
                                                    egui::RichText::new(&c.image)
                                                        .size(11.0)
                                                        .color(p.dim),
                                                );
                                                ui.label(if c.ready {
                                                    "prêt"
                                                } else {
                                                    "pas prêt"
                                                });
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
                                        let follow_key =
                                            format!("k8s:{}/{}", pod.namespace, pod.name);
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
                                        if ui
                                            .button("🔍")
                                            .on_hover_text("Fiche détaillée (conditions, volumes…)")
                                            .clicked()
                                        {
                                            app.pod_detail = None;
                                            app.page = Page::PodDetail {
                                                namespace: pod.namespace.clone(),
                                                name: pod.name.clone(),
                                            };
                                        }
                                        if ui
                                            .button("🗑")
                                            .on_hover_text("Supprimer le pod")
                                            .clicked()
                                        {
                                            app.send(Command::DeletePod {
                                                namespace: pod.namespace.clone(),
                                                name: pod.name.clone(),
                                            });
                                        }
                                        if ui
                                            .button("📄")
                                            .on_hover_text("Voir / éditer le YAML")
                                            .clicked()
                                        {
                                            app.yaml_loaded_key.clear();
                                            app.page = Page::YamlEditor {
                                                kind: "pods".into(),
                                                namespace: pod.namespace.clone(),
                                                name: pod.name.clone(),
                                            };
                                        }
                                    });
                                });
                        });
                        ui.end_row();
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
                .num_columns(6)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Déploiement");
                    ui.strong("Replicas");
                    ui.strong("Image");
                    ui.strong("Actions");
                    ui.strong("YAML");
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

                        if ui
                            .button("📄")
                            .on_hover_text("Voir / éditer le YAML")
                            .clicked()
                        {
                            app.yaml_loaded_key.clear();
                            app.page = Page::YamlEditor {
                                kind: "deployments".into(),
                                namespace: d.namespace.clone(),
                                name: d.name.clone(),
                            };
                        }
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
                    .num_columns(5)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Namespace");
                        ui.strong("Nom");
                        ui.strong("Détail");
                        ui.strong("Âge");
                        ui.strong("YAML");
                        ui.end_row();

                        for r in &items {
                            ui.label(egui::RichText::new(&r.namespace).color(p.dim));
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&r.name).strong());
                                crate::views::copy_btn(ui, &r.name);
                            });
                            ui.label(egui::RichText::new(&r.summary).size(12.0));
                            ui.label(egui::RichText::new(&r.age).size(12.0).color(p.dim));
                            if ui
                                .button("📄")
                                .on_hover_text("Voir / éditer le YAML")
                                .clicked()
                            {
                                app.yaml_loaded_key.clear();
                                app.page = Page::YamlEditor {
                                    kind: kind.to_string(),
                                    namespace: r.namespace.clone(),
                                    name: r.name.clone(),
                                };
                            }
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

// ---------------------------------------------------------------------------
// Workloads (StatefulSets, DaemonSets, ReplicaSets, Jobs, CronJobs)
// ---------------------------------------------------------------------------

pub fn workloads(app: &mut NaveoApp, ui: &mut egui::Ui, kind: &str) {
    let p = app.palette;
    let (title, empty_msg) = match kind {
        "statefulsets" => ("StatefulSets", "Aucun StatefulSet"),
        "daemonsets" => ("DaemonSets", "Aucun DaemonSet"),
        "replicasets" => ("ReplicaSets", "Aucun ReplicaSet"),
        "jobs" => ("Jobs", "Aucun Job"),
        "cronjobs" => ("CronJobs", "Aucun CronJob"),
        _ => (kind, "Aucune ressource"),
    };
    let search = app.search.to_lowercase();

    let items: Vec<_> = app
        .workloads
        .get(kind)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| {
            ns_matches(app, &w.namespace)
                && (search.is_empty()
                    || w.name.to_lowercase().contains(&search)
                    || w.namespace.to_lowercase().contains(&search))
        })
        .collect();

    let namespaces: Vec<String> = {
        let mut v: Vec<String> = app
            .workloads
            .get(kind)
            .map(|list| list.iter().map(|w| w.namespace.clone()).collect())
            .unwrap_or_default();
        v.sort();
        v.dedup();
        v
    };

    page_header(ui, &p, title, &format!("{} ressource(s)", items.len()));

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    ns_selector(app, ui, namespaces);
    ui.add_space(4.0);

    // Workloads scalables / redémarrables selon le kind.
    let scalable = matches!(kind, "statefulsets" | "replicasets");
    let restartable = matches!(kind, "statefulsets" | "daemonsets");

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            if items.is_empty() {
                ui.label(egui::RichText::new(empty_msg).color(p.dim));
            } else {
                egui::Grid::new(format!("wl-{kind}-grid"))
                    .num_columns(7)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Namespace");
                        ui.strong("Nom");
                        ui.strong("État");
                        ui.strong("Image");
                        ui.strong("Détail");
                        ui.strong("Âge");
                        ui.strong("Actions");
                        ui.end_row();

                        for w in &items {
                            ui.label(egui::RichText::new(&w.namespace).color(p.dim));
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&w.name).strong());
                                crate::views::copy_btn(ui, &w.name);
                            });

                            // Jobs : réussis / complétions ; CronJobs : pas de
                            // notion de réplicas ; sinon prêt / désiré.
                            if w.kind == "Job" || w.kind == "CronJob" {
                                ui.label(egui::RichText::new("—").color(p.dim));
                            } else {
                                let ok = w.replicas_ready == w.replicas_desired
                                    && w.replicas_desired > 0;
                                ui.horizontal(|ui| {
                                    ui.colored_label(if ok { p.ok } else { p.warn }, "●");
                                    ui.label(format!(
                                        "{}/{}",
                                        w.replicas_ready, w.replicas_desired
                                    ));
                                });
                            }

                            ui.label(egui::RichText::new(&w.image).size(11.0).color(p.dim));
                            ui.label(egui::RichText::new(&w.summary).size(12.0));
                            ui.label(egui::RichText::new(&w.age).size(12.0).color(p.dim));

                            ui.horizontal(|ui| {
                                if scalable {
                                    if ui
                                        .button("−")
                                        .on_hover_text("Réduire les réplicas")
                                        .clicked()
                                    {
                                        app.send(Command::ScaleWorkload {
                                            kind: kind.to_string(),
                                            namespace: w.namespace.clone(),
                                            name: w.name.clone(),
                                            replicas: (w.replicas_desired - 1).max(0),
                                        });
                                    }
                                    if ui
                                        .button("+")
                                        .on_hover_text("Augmenter les réplicas")
                                        .clicked()
                                    {
                                        app.send(Command::ScaleWorkload {
                                            kind: kind.to_string(),
                                            namespace: w.namespace.clone(),
                                            name: w.name.clone(),
                                            replicas: w.replicas_desired + 1,
                                        });
                                    }
                                }
                                if restartable
                                    && ui.button("↺").on_hover_text("Rollout restart").clicked()
                                {
                                    app.send(Command::RolloutRestartWorkload {
                                        kind: kind.to_string(),
                                        namespace: w.namespace.clone(),
                                        name: w.name.clone(),
                                    });
                                }
                                if ui
                                    .button("📄")
                                    .on_hover_text("Voir / éditer le YAML")
                                    .clicked()
                                {
                                    app.yaml_loaded_key.clear();
                                    app.page = Page::YamlEditor {
                                        kind: kind.to_string(),
                                        namespace: w.namespace.clone(),
                                        name: w.name.clone(),
                                    };
                                }
                                if ui.button("🗑").on_hover_text("Supprimer").clicked() {
                                    app.send(Command::DeleteWorkload {
                                        kind: kind.to_string(),
                                        namespace: w.namespace.clone(),
                                        name: w.name.clone(),
                                    });
                                }
                            });
                            ui.end_row();
                        }
                    });
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Fiche détaillée d'un pod
// ---------------------------------------------------------------------------

pub fn pod_detail(app: &mut NaveoApp, ui: &mut egui::Ui, ns: &str, name: &str) {
    let p = app.palette;
    page_header(ui, &p, &format!("Pod {ns}/{name}"), "Fiche détaillée");

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    if ui.button("← Retour aux pods").clicked() {
        app.page = Page::Pods;
        return;
    }
    ui.add_space(4.0);

    let Some(d) = app.pod_detail.clone() else {
        ui.spinner();
        ui.label(egui::RichText::new("Chargement de la fiche…").color(p.dim));
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Métadonnées principales.
        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("Métadonnées").strong());
            egui::Grid::new("poddetail-meta")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Phase");
                    ui.label(&d.phase);
                    ui.end_row();
                    if !d.qos.is_empty() {
                        ui.label("QoS");
                        ui.label(&d.qos);
                        ui.end_row();
                    }
                    if !d.service_account.is_empty() {
                        ui.label("ServiceAccount");
                        ui.label(&d.service_account);
                        ui.end_row();
                    }
                    if !d.node.is_empty() {
                        ui.label("Nœud");
                        ui.label(&d.node);
                        ui.end_row();
                    }
                    if !d.pod_ip.is_empty() {
                        ui.label("IP");
                        ui.label(egui::RichText::new(&d.pod_ip).monospace());
                        ui.end_row();
                    }
                    if !d.priority.is_empty() {
                        ui.label("Priorité");
                        ui.label(&d.priority);
                        ui.end_row();
                    }
                    if !d.owner_kind.is_empty() {
                        ui.label("Propriétaire");
                        // Clic → page du contrôleur propriétaire.
                        let owner_page = match d.owner_kind.as_str() {
                            "Deployment" => Some(Page::Deploiements),
                            "StatefulSet" => Some(Page::StatefulSets),
                            "DaemonSet" => Some(Page::DaemonSets),
                            "ReplicaSet" => Some(Page::ReplicaSets),
                            "Job" => Some(Page::Jobs),
                            "CronJob" => Some(Page::CronJobs),
                            _ => None,
                        };
                        match owner_page {
                            Some(page) => {
                                if ui
                                    .link(
                                        egui::RichText::new(format!(
                                            "{} {}",
                                            d.owner_kind, d.owner_name
                                        ))
                                        .color(p.accent),
                                    )
                                    .clicked()
                                {
                                    app.page = page;
                                }
                            }
                            None => {
                                ui.label(format!("{} {}", d.owner_kind, d.owner_name));
                            }
                        }
                        ui.end_row();
                    }
                });

            if !d.labels.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Labels").size(12.0).color(p.dim));
                egui::Grid::new("poddetail-labels")
                    .num_columns(2)
                    .spacing([14.0, 2.0])
                    .show(ui, |ui| {
                        for (k, v) in &d.labels {
                            ui.label(egui::RichText::new(k).size(11.0).color(p.dim));
                            ui.label(egui::RichText::new(v).size(11.0).monospace());
                            ui.end_row();
                        }
                    });
            }
            if !d.node_selector.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("nodeSelector").size(12.0).color(p.dim));
                for (k, v) in &d.node_selector {
                    ui.label(
                        egui::RichText::new(format!("{k}={v}"))
                            .size(11.0)
                            .monospace(),
                    );
                }
            }
        });
        ui.add_space(4.0);

        // Conditions du pod.
        if !d.conditions.is_empty() {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new("Conditions").strong());
                egui::Grid::new("poddetail-conditions")
                    .num_columns(4)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Type");
                        ui.strong("Statut");
                        ui.strong("Raison");
                        ui.strong("Message");
                        ui.end_row();
                        for c in &d.conditions {
                            ui.label(&c.type_);
                            let color = match c.status.as_str() {
                                "True" => p.ok,
                                "False" => p.err,
                                _ => p.warn,
                            };
                            ui.horizontal(|ui| {
                                ui.colored_label(color, "●");
                                ui.label(&c.status);
                            });
                            ui.label(
                                egui::RichText::new(if c.reason.is_empty() {
                                    "—"
                                } else {
                                    &c.reason
                                })
                                .size(12.0),
                            );
                            ui.label(egui::RichText::new(&c.message).size(12.0).color(p.dim));
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(4.0);
        }

        // Conteneurs : env, probes, ressources, montages.
        for c in &d.containers {
            theme::card(ui, &p, |ui| {
                let ccolor = theme::state_color(&p, &c.state);
                let header = format!(
                    "{}  ·  {}  ·  {}",
                    c.name,
                    c.state,
                    if c.ready { "prêt" } else { "pas prêt" }
                );
                egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                    .id_salt(format!("pd-{}-{}", d.namespace, c.name))
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(format!("pd-{}-meta", c.name))
                            .num_columns(2)
                            .spacing([14.0, 3.0])
                            .show(ui, |ui| {
                                ui.label("Image");
                                ui.label(egui::RichText::new(&c.image).size(11.0).monospace());
                                ui.end_row();
                                ui.label("État");
                                ui.horizontal(|ui| {
                                    ui.colored_label(ccolor, "●");
                                    ui.label(&c.state);
                                });
                                ui.end_row();
                                if !c.reason.is_empty() {
                                    ui.label("Raison");
                                    ui.label(&c.reason);
                                    ui.end_row();
                                }
                                ui.label("Redémarrages");
                                ui.label(c.restarts.to_string());
                                ui.end_row();
                            });

                        if !c.probes.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new("Sondes").size(12.0).color(p.dim));
                            for probe in &c.probes {
                                ui.label(egui::RichText::new(probe).size(11.0).monospace());
                            }
                        }
                        if !c.requests.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new("Requests").size(12.0).color(p.dim));
                            for (k, v) in &c.requests {
                                ui.label(
                                    egui::RichText::new(format!("{k} : {v}"))
                                        .size(11.0)
                                        .monospace(),
                                );
                            }
                        }
                        if !c.limits.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new("Limits").size(12.0).color(p.dim));
                            for (k, v) in &c.limits {
                                ui.label(
                                    egui::RichText::new(format!("{k} : {v}"))
                                        .size(11.0)
                                        .monospace(),
                                );
                            }
                        }
                        if !c.env.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new("Environnement").size(12.0).color(p.dim));
                            egui::Grid::new(format!("pd-{}-env", c.name))
                                .num_columns(2)
                                .spacing([14.0, 2.0])
                                .show(ui, |ui| {
                                    for (k, v) in &c.env {
                                        ui.label(egui::RichText::new(k).size(11.0).strong());
                                        ui.label(
                                            egui::RichText::new(v)
                                                .size(11.0)
                                                .monospace()
                                                .color(p.dim),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                        if !c.mounts.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new("Montages").size(12.0).color(p.dim));
                            for m in &c.mounts {
                                ui.label(egui::RichText::new(m).size(11.0).monospace());
                            }
                        }
                    });
            });
            ui.add_space(4.0);
        }

        // Volumes du pod.
        if !d.volumes.is_empty() {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new("Volumes").strong());
                egui::Grid::new("poddetail-volumes")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Nom");
                        ui.strong("Type");
                        ui.strong("Détail");
                        ui.end_row();
                        for v in &d.volumes {
                            ui.label(&v.name);
                            ui.label(&v.type_);
                            ui.label(egui::RichText::new(&v.detail).size(12.0).color(p.dim));
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(4.0);
        }

        // Tolérances.
        if !d.tolerations.is_empty() {
            theme::card(ui, &p, |ui| {
                ui.label(egui::RichText::new("Tolérances").strong());
                egui::Grid::new("poddetail-tolerations")
                    .num_columns(4)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Clé");
                        ui.strong("Opérateur");
                        ui.strong("Valeur");
                        ui.strong("Effet");
                        ui.end_row();
                        for t in &d.tolerations {
                            ui.label(&t.key);
                            ui.label(&t.operator);
                            ui.label(if t.value.is_empty() { "—" } else { &t.value });
                            ui.label(if t.effect.is_empty() {
                                "—"
                            } else {
                                &t.effect
                            });
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(4.0);
        }

        // Actions sur le pod.
        theme::card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                let follow_key = format!("k8s:{ns}/{name}");
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
                        app.page = Page::Journal;
                    }
                }
                if ui.button("⌨").on_hover_text("Terminal").clicked() {
                    app.exec_target = Some(format!("k8s:{ns}/{name}"));
                    app.page = Page::Terminal;
                }
                if ui.button("🗑").on_hover_text("Supprimer le pod").clicked() {
                    app.send(Command::DeletePod {
                        namespace: ns.to_string(),
                        name: name.to_string(),
                    });
                    app.page = Page::Pods;
                }
            });
        });
    });
}

// ---------------------------------------------------------------------------
// Éditeur YAML (ressource existante + création)
// ---------------------------------------------------------------------------

/// Zone d'édition YAML commune : boutons d'action + TextEdit monospace.
fn yaml_editor_common(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;

    ui.horizontal(|ui| {
        if ui.button("✔ Appliquer").clicked() {
            let yaml = app.yaml_text.clone();
            app.send(Command::ApplyYaml { yaml });
        }
        ui.label(
            egui::RichText::new(
                "Le YAML complet est envoyé au cluster (création ou remplacement).",
            )
            .size(11.0)
            .color(p.dim),
        );
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("yaml-editor-scroll")
        .show(ui, |ui| {
            let edit = egui::TextEdit::multiline(&mut app.yaml_text)
                .font(egui::TextStyle::Monospace)
                .code_editor()
                .desired_rows(28)
                .desired_width(f32::INFINITY);
            ui.add(edit);
        });
}

pub fn yaml_editor(app: &mut NaveoApp, ui: &mut egui::Ui, kind: &str, ns: &str, name: &str) {
    let p = app.palette;
    let titre = if ns.is_empty() {
        format!("YAML — {kind}/{name}")
    } else {
        format!("YAML — {kind}/{ns}/{name}")
    };
    page_header(ui, &p, &titre, "Édition de la ressource");

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    ui.horizontal(|ui| {
        if ui.button("← Retour").clicked() {
            app.page = page_for_kind(kind);
        }
        if ui.button("↺ Recharger").clicked() {
            app.yaml_loaded_key.clear();
            app.send(Command::GetResourceYaml {
                kind: kind.to_string(),
                namespace: ns.to_string(),
                name: name.to_string(),
            });
        }
    });
    ui.add_space(4.0);

    if app.yaml_loaded_key != format!("{kind}/{ns}/{name}") {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Chargement du YAML…").color(p.dim));
        });
    }
    yaml_editor_common(app, ui);
}

pub fn yaml_create(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    page_header(ui, &p, "Éditeur YAML", "Créer une ressource depuis un YAML");

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    ui.label(
        egui::RichText::new(
            "Colle un manifeste complet (apiVersion, kind, metadata.name requis) puis applique-le.",
        )
        .size(12.0)
        .color(p.dim),
    );
    ui.add_space(4.0);
    yaml_editor_common(app, ui);
}

// ---------------------------------------------------------------------------
// Bindings RBAC (RoleBindings + ClusterRoleBindings)
// ---------------------------------------------------------------------------

pub fn bindings(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let search = app.search.to_lowercase();

    let rb: Vec<_> = app
        .resources
        .get("rolebindings")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| {
            ns_matches(app, &r.namespace)
                && (search.is_empty() || r.name.to_lowercase().contains(&search))
        })
        .collect();
    let crb: Vec<_> = app
        .resources
        .get("clusterrolebindings")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| {
            ns_matches(app, &r.namespace)
                && (search.is_empty() || r.name.to_lowercase().contains(&search))
        })
        .collect();

    page_header(
        ui,
        &p,
        "Bindings RBAC",
        &format!(
            "{} rolebinding(s) · {} clusterrolebinding(s)",
            rb.len(),
            crb.len()
        ),
    );

    if !app.k8s_ok {
        ui.colored_label(p.warn, "Kubernetes n'est pas connecté.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("RoleBindings").strong());
            if rb.is_empty() {
                ui.label(egui::RichText::new("Aucun rolebinding").color(p.dim));
            } else {
                egui::Grid::new("rb-grid")
                    .num_columns(5)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Namespace");
                        ui.strong("Nom");
                        ui.strong("Détail");
                        ui.strong("Âge");
                        ui.strong("YAML");
                        ui.end_row();
                        for r in &rb {
                            ui.label(egui::RichText::new(&r.namespace).color(p.dim));
                            ui.label(egui::RichText::new(&r.name).strong());
                            ui.label(egui::RichText::new(&r.summary).size(12.0));
                            ui.label(egui::RichText::new(&r.age).size(12.0).color(p.dim));
                            if ui.button("📄").clicked() {
                                app.yaml_loaded_key.clear();
                                app.page = Page::YamlEditor {
                                    kind: "rolebindings".into(),
                                    namespace: r.namespace.clone(),
                                    name: r.name.clone(),
                                };
                            }
                            ui.end_row();
                        }
                    });
            }
        });
        ui.add_space(4.0);

        theme::card(ui, &p, |ui| {
            ui.label(egui::RichText::new("ClusterRoleBindings").strong());
            if crb.is_empty() {
                ui.label(egui::RichText::new("Aucun clusterrolebinding").color(p.dim));
            } else {
                egui::Grid::new("crb-grid")
                    .num_columns(5)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Namespace");
                        ui.strong("Nom");
                        ui.strong("Détail");
                        ui.strong("Âge");
                        ui.strong("YAML");
                        ui.end_row();
                        for r in &crb {
                            ui.label(egui::RichText::new(&r.namespace).color(p.dim));
                            ui.label(egui::RichText::new(&r.name).strong());
                            ui.label(egui::RichText::new(&r.summary).size(12.0));
                            ui.label(egui::RichText::new(&r.age).size(12.0).color(p.dim));
                            if ui.button("📄").clicked() {
                                app.yaml_loaded_key.clear();
                                app.page = Page::YamlEditor {
                                    kind: "clusterrolebindings".into(),
                                    namespace: r.namespace.clone(),
                                    name: r.name.clone(),
                                };
                            }
                            ui.end_row();
                        }
                    });
            }
        });
    });
}
