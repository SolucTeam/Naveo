//! État global de l'application, navigation et boucle principale egui.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;
use naveo_core::Event;

use crate::theme::{self, Palette};
use crate::views;

/// Pages de la sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Conteneurs,
    Images,
    Volumes,
    Reseaux,
    Noeuds,
    Pods,
    Deploiements,
    Journal,
    Parametres,
}

pub struct NaveoApp {
    pub(crate) cmd_tx: tokio::sync::mpsc::UnboundedSender<naveo_core::Command>,
    pub(crate) evt_rx: crossbeam_channel::Receiver<Event>,

    pub(crate) page: Page,
    pub(crate) palette: Palette,
    pub(crate) dark: bool,

    pub(crate) docker_ok: bool,
    pub(crate) k8s_ok: bool,
    pub(crate) k8s_ctx: String,

    pub(crate) containers: Vec<naveo_core::ContainerInfo>,
    pub(crate) images: Vec<naveo_core::ImageInfo>,
    pub(crate) volumes: Vec<naveo_core::VolumeInfo>,
    pub(crate) networks: Vec<naveo_core::NetworkInfo>,
    pub(crate) nodes: Vec<naveo_core::NodeInfo>,
    pub(crate) pods: Vec<naveo_core::PodInfo>,
    pub(crate) deployments: Vec<naveo_core::DeploymentInfo>,

    pub(crate) logs: HashMap<String, String>,
    pub(crate) log_target: Option<String>,

    pub(crate) search: String,
    pub(crate) error: Option<String>,
    pub(crate) last_refresh: Instant,
    pub(crate) last_error: Option<Instant>,
}

impl NaveoApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        cmd_tx: tokio::sync::mpsc::UnboundedSender<naveo_core::Command>,
        evt_rx: crossbeam_channel::Receiver<Event>,
    ) -> Self {
        Self {
            cmd_tx,
            evt_rx,
            page: Page::Conteneurs,
            palette: theme::dark(),
            dark: true,
            docker_ok: false,
            k8s_ok: false,
            k8s_ctx: String::new(),
            containers: Vec::new(),
            images: Vec::new(),
            volumes: Vec::new(),
            networks: Vec::new(),
            nodes: Vec::new(),
            pods: Vec::new(),
            deployments: Vec::new(),
            logs: HashMap::new(),
            log_target: None,
            search: String::new(),
            error: None,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_error: None,
        }
    }

    pub(crate) fn send(&self, cmd: naveo_core::Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub(crate) fn refresh_visible(&self) {
        self.send(naveo_core::Command::RefreshContainers);
        self.send(naveo_core::Command::RefreshImages);
        match self.page {
            Page::Noeuds | Page::Pods | Page::Deploiements => {
                self.send(naveo_core::Command::RefreshK8s);
            }
            _ => {}
        }
    }

    /// Consomme les événements poussés par le backend.
    fn poll_events(&mut self) {
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Event::Connected { docker, kubernetes, context } => {
                    self.docker_ok = docker;
                    self.k8s_ok = kubernetes;
                    self.k8s_ctx = context;
                    self.refresh_visible();
                }
                Event::Containers(c) => self.containers = c,
                Event::Images(i) => self.images = i,
                Event::Volumes(v) => self.volumes = v,
                Event::Networks(n) => self.networks = n,
                Event::Nodes(n) => self.nodes = n,
                Event::Pods(p) => self.pods = p,
                Event::Deployments(d) => self.deployments = d,
                Event::Logs { target, text } => {
                    self.logs.insert(target.clone(), text);
                    self.log_target = Some(target);
                }
                Event::Error(e) => {
                    self.error = Some(e);
                    self.last_error = Some(Instant::now());
                }
            }
        }
    }
}

impl eframe::App for NaveoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Rafraîchit la palette si le thème a changé.
        if self.dark != self.palette.dark {
            self.palette = if self.dark { theme::dark() } else { theme::light() };
        }
        theme::apply(ctx, &self.palette);

        self.poll_events();

        // Auto-rafraîchissement toutes les 5 s.
        if self.last_refresh.elapsed() > Duration::from_secs(5) {
            self.refresh_visible();
            self.last_refresh = Instant::now();
        }

        // Fait redessiner egui régulièrement (données poussées par canal).
        ctx.request_repaint_after(Duration::from_millis(250));

        self.draw_sidebar(ctx);
        self.draw_topbar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(self.palette.bg).inner_margin(egui::Margin::symmetric(16, 12)))
            .show(ctx, |ui| match self.page {
                Page::Conteneurs => views::containers::show(self, ui),
                Page::Images => views::images::show(self, ui),
                Page::Volumes => views::volumes::show(self, ui),
                Page::Reseaux => views::networks::show(self, ui),
                Page::Noeuds => views::k8s::nodes(self, ui),
                Page::Pods => views::k8s::pods(self, ui),
                Page::Deploiements => views::k8s::deployments(self, ui),
                Page::Journal => views::logs::show(self, ui),
                Page::Parametres => views::settings::show(self, ui),
            });

        // Toast d'erreur (disparaît après 6 s).
        if let (Some(err), Some(when)) = (self.error.clone(), self.last_error) {
            if when.elapsed() < Duration::from_secs(6) {
                egui::Window::new("⚠ Erreur")
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.colored_label(self.palette.err, &err);
                    });
            } else {
                self.error = None;
            }
        }
    }
}

impl NaveoApp {
    fn draw_sidebar(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        egui::SidePanel::left("sidebar")
            .exact_width(190.0)
            .frame(egui::Frame::default().fill(p.panel).inner_margin(egui::Margin::same(10)))
            .show(ctx, |ui| {
                ui.add_space(6.0);

                // Logo / titre
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("◆ Naveo")
                            .size(20.0)
                            .strong()
                            .color(p.accent),
                    );
                });
                ui.label(
                    egui::RichText::new("conteneurs · kubernetes")
                        .size(11.0)
                        .color(p.dim),
                );
                ui.add_space(12.0);

                // État des connexions
                if p.dark {
                    // rien, la carte de statut est en haut à droite
                }
                ui.separator();
                ui.add_space(4.0);

                self.nav_item(ui, "📦", "Conteneurs", Page::Conteneurs);
                self.nav_item(ui, "🖼", "Images", Page::Images);
                self.nav_item(ui, "💾", "Volumes", Page::Volumes);
                self.nav_item(ui, "🕸", "Réseaux", Page::Reseaux);

                ui.add_space(10.0);
                ui.label(egui::RichText::new("KUBERNETES").size(10.0).color(p.dim).strong());
                ui.add_space(2.0);
                self.nav_item(ui, "🖥", "Nœuds", Page::Noeuds);
                self.nav_item(ui, "🫧", "Pods", Page::Pods);
                self.nav_item(ui, "🚀", "Déploiements", Page::Deploiements);

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    ui.separator();
                    self.nav_item(ui, "📜", "Journal", Page::Journal);
                    self.nav_item(ui, "⚙️", "Paramètres", Page::Parametres);
                });
            });
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, icon: &str, label: &str, page: Page) {
        let p = self.palette;
        let selected = self.page == page;
        let text = format!("{icon}  {label}");
        let resp = ui.selectable_label(selected, egui::RichText::new(&text).size(14.0).color(if selected { p.accent } else { p.text }));
        if resp.clicked() {
            self.page = page;
        }
    }

    fn draw_topbar(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        egui::TopBottomPanel::top("topbar")
            .frame(egui::Frame::default().fill(p.panel).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Recherche
                    let search = ui.add_sized(
                        [280.0, 24.0],
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("🔍  Filtrer…"),
                    );
                    if search.changed() {
                        // Le filtre est relu par les vues.
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Carte de statut des connexions.
                        status_chip(ui, p, "Docker", self.docker_ok);
                        status_chip(ui, p, "Kubernetes", self.k8s_ok);
                        if !self.k8s_ctx.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("· {}", self.k8s_ctx))
                                    .size(11.0)
                                    .color(p.dim),
                            );
                        }
                        if ui.button("↻").clicked() {
                            self.refresh_visible();
                        }
                    });
                });
            });
    }
}

fn status_chip(ui: &mut egui::Ui, p: Palette, label: &str, ok: bool) {
    let (color, glyph) = if ok { (p.ok, "●") } else { (p.dim, "○") };
    egui::Frame::default()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.border))
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, glyph);
                ui.label(egui::RichText::new(label).size(12.0));
            });
        });
}
