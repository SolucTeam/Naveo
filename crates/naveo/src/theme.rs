//! Thème Naveo : palette inspirée de Kiri (sombre, calme, accents verts),
//! cartes arrondies et espacement aéré.

use egui::{vec2, Color32, Context, CornerRadius, Style};

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Fond de la fenêtre.
    pub bg: Color32,
    /// Fond des panneaux (sidebar, barre du haut).
    pub panel: Color32,
    /// Fond des cartes.
    pub card: Color32,
    /// Fond des zones de code / logs.
    pub code: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
}

pub fn dark() -> Palette {
    Palette {
        dark: true,
        bg: Color32::from_rgb(17, 20, 24),
        panel: Color32::from_rgb(23, 27, 32),
        card: Color32::from_rgb(30, 35, 41),
        code: Color32::from_rgb(13, 15, 18),
        border: Color32::from_rgb(44, 51, 59),
        text: Color32::from_rgb(226, 232, 240),
        dim: Color32::from_rgb(139, 150, 163),
        accent: Color32::from_rgb(52, 211, 153), // émeraude
        ok: Color32::from_rgb(52, 211, 153),
        warn: Color32::from_rgb(251, 191, 36),
        err: Color32::from_rgb(248, 113, 113),
    }
}

pub fn light() -> Palette {
    Palette {
        dark: false,
        bg: Color32::from_rgb(246, 248, 250),
        panel: Color32::from_rgb(240, 243, 246),
        card: Color32::WHITE,
        code: Color32::from_rgb(24, 27, 31),
        border: Color32::from_rgb(219, 225, 231),
        text: Color32::from_rgb(30, 38, 47),
        dim: Color32::from_rgb(110, 122, 135),
        accent: Color32::from_rgb(5, 150, 105),
        ok: Color32::from_rgb(5, 150, 105),
        warn: Color32::from_rgb(180, 130, 10),
        err: Color32::from_rgb(200, 60, 60),
    }
}

/// Applique la palette au contexte egui.
pub fn apply(ctx: &Context, p: &Palette) {
    let mut style = Style::default();

    style.visuals.dark_mode = p.dark;
    style.visuals.panel_fill = p.panel;
    style.visuals.window_fill = p.card;
    style.visuals.extreme_bg_color = p.code;
    style.visuals.override_text_color = Some(p.text);
    style.visuals.selection.bg_fill = p.accent.gamma_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);

    let rounding = CornerRadius::same(8);
    style.visuals.window_corner_radius = CornerRadius::same(12);
    style.visuals.menu_corner_radius = CornerRadius::same(8);
    style.visuals.widgets.noninteractive.corner_radius = rounding;
    style.visuals.widgets.inactive.corner_radius = rounding;
    style.visuals.widgets.hovered.corner_radius = rounding;
    style.visuals.widgets.active.corner_radius = rounding;
    style.visuals.widgets.open.corner_radius = rounding;

    style.visuals.widgets.inactive.bg_fill = p.card;
    style.visuals.widgets.inactive.weak_bg_fill = p.card;
    style.visuals.widgets.hovered.weak_bg_fill = p.border;
    style.visuals.widgets.active.weak_bg_fill = p.border;

    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.border);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, p.border);

    style.spacing.item_spacing = vec2(10.0, 8.0);
    style.spacing.button_padding = vec2(10.0, 5.0);
    style.spacing.menu_margin = egui::Margin::same(6);

    ctx.set_style(style);
}

/// Dessine le contenu dans une carte arrondie.
pub fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::default()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Point de statut coloré + libellé.
#[allow(dead_code)]
pub fn status_dot(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(label);
    });
}

/// Couleur selon l'état d'un conteneur / pod.
pub fn state_color(p: &Palette, state: &str) -> Color32 {
    match state.to_lowercase().as_str() {
        "running" => p.ok,
        "exited" | "succeeded" | "completed" => p.dim,
        "paused" | "pending" | "containercreating" => p.warn,
        "failed" | "crashloopbackoff" | "error" | "dead" => p.err,
        _ => p.dim,
    }
}

/// Taille lisible : 1 234 → « 1,2 Ko ».
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
