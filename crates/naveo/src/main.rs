//! Naveo — point d'entrée : lance le runtime asynchrone (backend)
//! et la fenêtre native egui (frontend), reliés par des canaux.

mod app;
mod theme;
mod views;

use app::NaveoApp;
use naveo_core::{Command, Event};

fn main() -> eframe::Result<()> {
    // Runtime async pour le backend (Docker + Kubernetes).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("impossible de créer le runtime tokio");

    // Canaux : Commandes UI -> backend, Événements backend -> UI.
    let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<Event>();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();

    // Tâche de fond : se connecte, puis traite les commandes une à une.
    rt.spawn(async move {
        let docker = naveo_core::docker::connect().await.ok();
        let mut k8s = naveo_core::k8s::connect().await.ok();
        let docker_clone = docker.clone();
        let mut follows: std::collections::HashMap<String, tokio::task::AbortHandle> =
            std::collections::HashMap::new();

        let _ = evt_tx.send(Event::Connected {
            docker: docker.is_some(),
            kubernetes: k8s.is_some(),
            context: k8s.as_ref().map(|(_, ctx)| ctx.clone()).unwrap_or_default(),
        });

        // Contextes découverts dans les kubeconfigs locaux (multi-cluster).
        let _ = evt_tx.send(Event::ContextsAvailable(naveo_core::k8s::list_contexts()));

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::ConnectContext { file, context } => {
                    match naveo_core::k8s::connect_to(&file, &context).await {
                        Ok((client, ctx)) => {
                            k8s = Some((client, ctx.clone()));
                            let _ = evt_tx.send(Event::K8sConnected { context: ctx });
                        }
                        Err(e) => {
                            let _ =
                                evt_tx.send(Event::Error(format!("Connexion à {context} : {e}")));
                        }
                    }
                }
                // Flux en direct (logs/stats) : tâches annulables.
                Command::FollowLogs { key } => {
                    if let Some(h) = follows.remove(&key) {
                        h.abort();
                    }
                    let tx2 = evt_tx.clone();
                    let key2 = key.clone();
                    let docker2 = docker_clone.clone();
                    let k8s2 = k8s.clone();
                    let handle = tokio::spawn(async move {
                        let _ = if let Some(name) = key2.strip_prefix("docker:") {
                            match docker2 {
                                Some(d) => {
                                    naveo_core::docker::stream_logs(&d, name, &key2, tx2).await
                                }
                                None => Ok(()),
                            }
                        } else if let Some(rest) = key2.strip_prefix("k8s:") {
                            match k8s2.map(|(c, _)| c) {
                                Some(client) => {
                                    // clé : k8s:<ns>/<name>[/<container>]
                                    let mut parts = rest.splitn(3, '/');
                                    let ns = parts.next().unwrap_or_default().to_string();
                                    let pod = parts.next().unwrap_or_default().to_string();
                                    let container = parts.next().map(|s| s.to_string());
                                    naveo_core::k8s::stream_pod_logs(
                                        &client,
                                        &ns,
                                        &pod,
                                        container.as_deref(),
                                        &key2,
                                        tx2,
                                    )
                                    .await
                                }
                                None => Ok(()),
                            }
                        } else {
                            Ok(())
                        };
                    });
                    follows.insert(key, handle.abort_handle());
                }
                Command::FollowStats(name) => {
                    let key = format!("stats:{name}");
                    if let Some(h) = follows.remove(&key) {
                        h.abort();
                    }
                    if let Some(d) = docker_clone.clone() {
                        let tx2 = evt_tx.clone();
                        let handle = tokio::spawn(async move {
                            naveo_core::docker::stream_stats(&d, &name, tx2).await
                        });
                        follows.insert(key, handle.abort_handle());
                    }
                }
                Command::StopFollow { key } => {
                    if let Some(h) = follows.remove(&key) {
                        h.abort();
                    }
                }
                other => match naveo_core::dispatch(
                    other,
                    docker.as_ref(),
                    k8s.as_ref().map(|(c, _)| c),
                    evt_tx.clone(),
                )
                .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        let _ = evt_tx.send(Event::Error(e.to_string()));
                    }
                },
            }
        }
    });

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/logo.png"))
        .expect("chargement de l'icône Naveo");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Naveo")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "Naveo",
        options,
        Box::new(|cc| {
            let app = NaveoApp::new(cc, cmd_tx, evt_rx);
            Ok(Box::new(app))
        }),
    )
}
