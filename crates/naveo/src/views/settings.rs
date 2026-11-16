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
        ui.checkbox(&mut app.dark, "Thème sombre");
        ui.label(
            egui::RichText::new("Le thème s'applique immédiatement.")
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
                 Kubernetes : contexte par défaut du KUBECONFIG.",
            )
            .size(12.0)
            .color(p.dim),
        );
    });
    ui.add_space(8.0);

    // À propos
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new("À propos").strong());
        ui.add_space(4.0);
        ui.label("Naveo v0.1.0 — conteneurs + Kubernetes, une seule app native en Rust.");
        ui.label(
            egui::RichText::new(
                "Interface inspirée de Kiri · fonctionnalités Kubernetes inspirées de Headlamp.",
            )
            .size(12.0)
            .color(p.dim),
        );
    });
}
