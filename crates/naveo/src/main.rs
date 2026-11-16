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
