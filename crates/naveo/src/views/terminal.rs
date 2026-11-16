//! Vue Terminal — exécution de commandes dans les conteneurs Docker
//! et les pods Kubernetes (`sh -c <cmd>`, sortie combinée).

use eframe::egui;
use naveo_core::Command;

use crate::app::{NaveoApp, Page};
use crate::theme;
use crate::views::page_header;

/// Liste les cibles disponibles : conteneurs actifs + pods.
fn available_targets(app: &NaveoApp) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();

    for c in &app.containers {
        if c.state == "running" {
            targets.push(format!("docker:{}", c.name));
        }
    }
    for p in &app.pods {
        if p.phase == "Running" && !p.containers.is_empty() {
            targets.push(format!("k8s:{}/{}", p.namespace, p.name));
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

pub fn show(app: &mut NaveoApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let targets = available_targets(app);

    page_header(
        ui,
        &p,
        "Terminal",
        "sh -c <cmd> dans un conteneur ou un pod",
    );

    if targets.is_empty() {
        ui.label(
            egui::RichText::new(
                "Aucune cible disponible. Lance un conteneur Docker ou attends des pods Running.",
            )
            .color(p.dim),
        );
        return;
    }

    // Sélection de la cible.
    if app.exec_target.is_none() || !targets.contains(app.exec_target.as_ref().unwrap()) {
        app.exec_target = Some(targets[0].clone());
    }

    ui.horizontal(|ui| {
        ui.label("Cible :");
        egui::ComboBox::from_id_salt("exec-target")
            .selected_text(app.exec_target.as_deref().unwrap_or("—"))
            .width(380.0)
            .show_ui(ui, |ui| {
                for t in &targets {
                    ui.selectable_value(&mut app.exec_target, Some(t.clone()), t);
                }
            });

        if ui.button("🧹 Effacer").clicked() {
            if let Some(t) = &app.exec_target {
                app.exec_out.remove(t);
            }
        }
    });
    ui.add_space(8.0);

    // Zone de sortie + saisie.
    let target = app.exec_target.clone().unwrap();
    let target_for_ui = target.clone();

    theme::card(ui, &p, |ui| {
        // Sortie accumulée (scroll, collé en bas).
        let text = app
            .exec_out
            .entry(target_for_ui.clone())
            .or_insert_with(|| format!("❯ terminal {target_for_ui}\n"));
        let text = text.clone();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut text.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(22)
                        .desired_width(ui.available_width())
                        .interactive(true),
                );
            });

        ui.add_space(6.0);
        ui.separator();

        // Ligne de commande : Entrée ou bouton ▶ pour exécuter.
        ui.horizontal(|ui| {
            ui.colored_label(p.accent, "❯");

            let mut cmd = app
                .exec_out
                .get(&format!("_pending::{target_for_ui}"))
                .cloned()
                .unwrap_or_default();

            let response = ui.add(
                egui::TextEdit::singleline(&mut cmd)
                    .hint_text("ex. ls -la /  ·  ps aux  ·  cat /etc/hostname")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(ui.available_width() - 120.0),
            );

            let run = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let run = run
                || ui
                    .add_enabled(!app.exec_busy, egui::Button::new("▶ Lancer"))
                    .clicked();

            if run && !cmd.trim().is_empty() && !app.exec_busy {
                // Mémorise la commande affichée + vide la ligne.
                let entry = app.exec_out.entry(target_for_ui.clone()).or_default();
                entry.push_str(&format!("❯ {cmd}\n"));

                let target = target_for_ui.clone();
                let cmd_to_run = cmd.trim().to_string();
                app.exec_out
                    .insert(format!("_pending::{target}"), String::new());
                app.exec_busy = true;

                if let Some(name) = target.strip_prefix("docker:") {
                    app.send(Command::ExecContainer {
                        name: name.to_string(),
                        cmd: cmd_to_run,
                    });
                } else if let Some(rest) = target.strip_prefix("k8s:") {
                    let (ns, pod) = rest.split_once('/').unwrap_or((rest, rest));
                    app.send(Command::PodExec {
                        namespace: ns.to_string(),
                        name: pod.to_string(),
                        container: String::new(),
                        cmd: cmd_to_run,
                    });
                }
                // Renvoie le focus sur la ligne de commande.
                response.request_focus();
                app.page = Page::Terminal;
            }

            // Conserve le texte en cours de saisie entre les frames.
            app.exec_out
                .insert(format!("_pending::{target_for_ui}"), cmd);
        });
    });

    if app.exec_busy {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("⏳ Commande en cours…")
                .size(12.0)
                .color(p.dim),
        );
    }
}
