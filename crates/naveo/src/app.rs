//! État global de l'application, navigation et boucle principale egui.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use eframe::egui;
use naveo_core::Event;

use crate::theme::{self, Palette};
use crate::views;

/// Pages de la sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Conteneurs,
    Images,
    Volumes,
    Reseaux,
    Noeuds,
    Pods,
    Services,
    Deploiements,
    StatefulSets,
    DaemonSets,
    ReplicaSets,
    Jobs,
    CronJobs,
    Evenements,
    ConfigMaps,
    Secrets,
    Ingress,
    Endpoints,
    Stockage,
    PVs,
    StorageClasses,
    ServiceAccounts,
    Roles,
    ClusterRoles,
    Bindings,
    Namespaces,
    /// Page détail d'un conteneur Docker (logs live, stats, inspect, fichiers).
    ContainerDetail(String),
    /// Fiche détaillée d'un pod (conditions, volumes, probes…).
    PodDetail {
        namespace: String,
        name: String,
    },
    /// Éditeur YAML d'une ressource existante.
    YamlEditor {
        kind: String,
        namespace: String,
        name: String,
    },
    /// Création d'une ressource depuis un YAML collé.
    YamlCreate,
    Terminal,
    Journal,
    Parametres,
}

/// Mode de thème : sombre, clair ou suivre le système.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeMode {
    Sombre,
    Clair,
    Systeme,
}

impl ThemeMode {
    fn key(self) -> &'static str {
        match self {
            ThemeMode::Sombre => "dark",
            ThemeMode::Clair => "light",
            ThemeMode::Systeme => "system",
        }
    }
    fn from_key(s: &str) -> Self {
        match s {
            "light" => ThemeMode::Clair,
            "system" => ThemeMode::Systeme,
            _ => ThemeMode::Sombre,
        }
    }
}

/// Un port-forward actif.
#[derive(Debug, Clone)]
pub(crate) struct ForwardInfo {
    pub key: String,
    pub namespace: String,
    pub pod: String,
    pub local_port: u16,
    pub target_port: u16,
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

    // Terminal exec
    pub(crate) exec_out: HashMap<String, String>,
    pub(crate) exec_target: Option<String>,
    pub(crate) exec_busy: bool,

    // Multi-cluster
    pub(crate) contexts: Vec<naveo_core::ContextInfo>,

    // Détail conteneur / flux en direct
    pub(crate) stats: HashMap<String, naveo_core::StatsInfo>,
    pub(crate) inspect: HashMap<String, String>,
    pub(crate) following: HashSet<String>,
    pub(crate) log_filter: String,
    /// Filtre de namespace Kubernetes (Tous si None).
    pub(crate) ns_filter: Option<String>,
    pub(crate) services: Vec<naveo_core::ServiceInfo>,
    prev_page: Option<Page>,

    // Événements Kubernetes + ressources génériques
    pub(crate) k8s_events: Vec<naveo_core::K8sEventInfo>,
    pub(crate) resources: HashMap<String, Vec<naveo_core::ResourceInfo>>,

    // Workloads (statefulsets, daemonsets, replicasets, jobs, cronjobs)
    pub(crate) workloads: HashMap<String, Vec<naveo_core::WorkloadInfo>>,

    // Fiche pod détaillée
    pub(crate) pod_detail: Option<naveo_core::PodDetailInfo>,

    // Métriques metrics-server (« ns/name » ou nom de nœud → (milliCPU, octets))
    pub(crate) pod_metrics: HashMap<String, (u64, u64)>,
    pub(crate) node_metrics: HashMap<String, (u64, u64)>,

    // Sélection multiple de pods (actions en masse K8s)
    pub(crate) sel_pods: HashSet<String>,

    // Éditeur YAML
    /// Clé de la dernière version chargée (évite les rechargements en boucle).
    pub(crate) yaml_loaded_key: String,
    pub(crate) yaml_text: String,

    // Port-forwards actifs
    pub(crate) forwards: Vec<ForwardInfo>,
    pub(crate) pf_ns: String,
    pub(crate) pf_pod: String,
    pub(crate) pf_local: String,
    pub(crate) pf_target: String,

    // Images : pull / Docker Hub / run
    pub(crate) pull_field: String,
    pub(crate) hub_query: String,
    pub(crate) hub_results: Vec<naveo_core::HubRepo>,
    pub(crate) hub_tags: HashMap<String, Vec<String>>,
    pub(crate) hub_open_repo: Option<String>,
    pub(crate) run_open: bool,
    pub(crate) run_image: String,
    pub(crate) run_name: String,
    pub(crate) run_env: Vec<(String, String)>,
    pub(crate) run_ports: Vec<(String, String)>,
    pub(crate) image_history: HashMap<String, Vec<naveo_core::ImageLayerInfo>>,
    pub(crate) history_open: Option<String>,
    pub(crate) tag_field: String,
    /// Image en attente d'un nouveau tag (dialogue ouvert).
    pub(crate) pending_tag: Option<String>,
    /// Déclenche le sélecteur de fichier d'import d'image.
    pub(crate) load_open: bool,
    pub(crate) registry_open: bool,
    pub(crate) reg_server: String,
    pub(crate) reg_user: String,
    pub(crate) reg_pass: String,

    // Sélection multiple (actions en masse) + prune
    pub(crate) sel_containers: HashSet<String>,
    pub(crate) sel_images: HashSet<String>,
    pub(crate) prune_confirm: Option<String>,

    // Fichiers du conteneur (explorateur)
    pub(crate) files_name: Option<String>,
    pub(crate) files_path: String,
    pub(crate) files_entries: Vec<naveo_core::ContainerFile>,
    pub(crate) file_preview: Option<(String, String)>, // (chemin, contenu)

    // Édition à chaud (détail conteneur)
    pub(crate) rename_pending: Option<String>,
    pub(crate) rename_field: String,
    pub(crate) commit_pending: Option<String>,
    pub(crate) commit_field: String,
    pub(crate) edit_container: Option<String>,
    pub(crate) edit_env: Vec<(String, String)>,
    pub(crate) edit_ports: Vec<(String, String)>,

    // Logs enrichis
    pub(crate) log_timestamps: bool,
    pub(crate) log_tail: i64,
    pub(crate) log_level: String,

    // Terminal : historique ↑↓
    pub(crate) cmd_history: Vec<String>,
    pub(crate) cmd_hist_idx: Option<usize>,

    // Tri de colonnes : (colonne, croissant)
    pub(crate) sort_containers: (usize, bool),
    pub(crate) sort_images: (usize, bool),

    // Thème et préférences persistées
    pub(crate) theme_mode: ThemeMode,

    pub(crate) search: String,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) last_refresh: Instant,
    pub(crate) last_error: Option<Instant>,
    pub(crate) last_notice: Option<Instant>,
}

impl NaveoApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        cmd_tx: tokio::sync::mpsc::UnboundedSender<naveo_core::Command>,
        evt_rx: crossbeam_channel::Receiver<Event>,
    ) -> Self {
        // Préférences persistées (thème, options de logs).
        let prefs = load_prefs();
        let theme_mode = ThemeMode::from_key(&prefs.theme);

        // Thème système : suit l'apparence de l'OS en mode « system ».
        let dark = match theme_mode {
            ThemeMode::Sombre => true,
            ThemeMode::Clair => false,
            ThemeMode::Systeme => dark_light::detect().is_ok_and(|m| m == dark_light::Mode::Dark),
        };

        Self {
            cmd_tx,
            evt_rx,
            page: Page::Conteneurs,
            palette: if dark { theme::dark() } else { theme::light() },
            dark,
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
            exec_out: HashMap::new(),
            exec_target: None,
            exec_busy: false,
            contexts: Vec::new(),
            stats: HashMap::new(),
            inspect: HashMap::new(),
            following: HashSet::new(),
            log_filter: String::new(),
            ns_filter: None,
            services: Vec::new(),
            prev_page: None,
            k8s_events: Vec::new(),
            resources: HashMap::new(),
            workloads: HashMap::new(),
            pod_detail: None,
            pod_metrics: HashMap::new(),
            node_metrics: HashMap::new(),
            sel_pods: HashSet::new(),
            yaml_loaded_key: String::new(),
            yaml_text: String::new(),
            forwards: Vec::new(),
            pf_ns: String::new(),
            pf_pod: String::new(),
            pf_local: String::new(),
            pf_target: String::new(),
            pull_field: String::new(),
            hub_query: String::new(),
            hub_results: Vec::new(),
            hub_tags: HashMap::new(),
            hub_open_repo: None,
            run_open: false,
            run_image: String::new(),
            run_name: String::new(),
            run_env: vec![(String::new(), String::new())],
            run_ports: vec![(String::new(), String::new())],
            image_history: HashMap::new(),
            history_open: None,
            tag_field: String::new(),
            pending_tag: None,
            load_open: false,
            registry_open: false,
            reg_server: String::new(),
            reg_user: String::new(),
            reg_pass: String::new(),
            sel_containers: HashSet::new(),
            sel_images: HashSet::new(),
            prune_confirm: None,
            files_name: None,
            files_path: String::new(),
            files_entries: Vec::new(),
            file_preview: None,
            rename_pending: None,
            rename_field: String::new(),
            commit_pending: None,
            commit_field: String::new(),
            edit_container: None,
            edit_env: vec![(String::new(), String::new())],
            edit_ports: vec![(String::new(), String::new())],
            log_timestamps: prefs.log_timestamps,
            log_tail: 500,
            log_level: String::new(),
            cmd_history: Vec::new(),
            cmd_hist_idx: None,
            sort_containers: (0, true),
            sort_images: (0, true),
            theme_mode,
            search: String::new(),
            error: None,
            notice: None,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_error: None,
            last_notice: None,
        }
    }

    pub(crate) fn send(&self, cmd: naveo_core::Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub(crate) fn refresh_visible(&self) {
        self.send(naveo_core::Command::RefreshContainers);
        self.send(naveo_core::Command::RefreshImages);
        if self.k8s_ok {
            self.send(naveo_core::Command::RefreshK8s);
            // Recharge les données de la page courante (workloads,
            // ressources en lecture, métriques).
            let workload = workload_page_kind(&self.page);
            if let Some(kind) = workload {
                self.send(naveo_core::Command::LoadWorkloads { kind: kind.into() });
            }
            if let Some(kind) = resource_page_kind(&self.page) {
                for k in kind {
                    self.send(naveo_core::Command::LoadResources {
                        kind: (*k).to_string(),
                    });
                }
            }
            match &self.page {
                Page::Noeuds | Page::Pods => {
                    self.send(naveo_core::Command::LoadMetrics);
                }
                Page::PodDetail { namespace, name } => {
                    self.send(naveo_core::Command::LoadMetrics);
                    self.send(naveo_core::Command::LoadPodDetail {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    /// Change le mode de thème et le persiste.
    pub(crate) fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.dark = match mode {
            ThemeMode::Sombre => true,
            ThemeMode::Clair => false,
            ThemeMode::Systeme => dark_light::detect().is_ok_and(|m| m == dark_light::Mode::Dark),
        };
        save_prefs(&prefs_of(self));
    }

    /// Ouvre un sélecteur de fichier et copie le kubeconfig dans ~/.kube/,
    /// puis rescanne les contextes (comme l'import de Headlamp).
    pub(crate) fn import_kubeconfig(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choisir un kubeconfig à importer")
            .pick_file()
        else {
            return;
        };

        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let kube_dir = std::path::PathBuf::from(home).join(".kube");
        if std::fs::create_dir_all(&kube_dir).is_err() {
            self.error = Some("Impossible de créer ~/.kube/".into());
            self.last_error = Some(Instant::now());
            return;
        }

        if let Some(name) = path.file_name() {
            let dest = kube_dir.join(name);
            if path != dest {
                if let Err(e) = std::fs::copy(&path, &dest) {
                    self.error = Some(format!("Import impossible : {e}"));
                    self.last_error = Some(Instant::now());
                    return;
                }
            }
        }

        self.send(naveo_core::Command::RefreshContexts);
    }

    /// Consomme les événements poussés par le backend.
    fn poll_events(&mut self) {
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Event::Connected {
                    docker,
                    kubernetes,
                    context,
                } => {
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
                Event::Services(s) => self.services = s,
                Event::LogChunk { target, text } => {
                    let entry = self.logs.entry(target.clone()).or_default();
                    entry.push_str(&text);
                    self.log_target = Some(target);
                }
                Event::Stats { name, info } => {
                    self.stats.insert(name, info);
                }
                Event::Inspect { target, json } => {
                    self.inspect.insert(target, json);
                }
                Event::ExecOutput { target, text } => {
                    let entry = self.exec_out.entry(target.clone()).or_default();
                    entry.push_str(&text);
                    if !text.ends_with('\n') {
                        entry.push('\n');
                    }
                    self.log_target = Some(target);
                    self.exec_busy = false;
                }
                Event::ContextsAvailable(list) => self.contexts = list,
                Event::K8sConnected { context } => {
                    self.k8s_ok = true;
                    self.k8s_ctx = context;
                    self.send(naveo_core::Command::RefreshK8s);
                }
                Event::HubResults(list) => self.hub_results = list,
                Event::HubTags { repo, tags } => {
                    self.hub_tags.insert(repo, tags);
                }
                Event::ImageHistory { image, layers } => {
                    self.image_history.insert(image, layers);
                }
                Event::FileList {
                    name,
                    path,
                    entries,
                } => {
                    self.files_name = Some(name);
                    self.files_path = path;
                    self.files_entries = entries;
                }
                Event::FilePreview { path, content, .. } => {
                    self.file_preview = Some((path, content));
                }
                Event::Events(list) => self.k8s_events = list,
                Event::Resources { kind, items } => {
                    self.resources.insert(kind, items);
                }
                Event::Workloads { kind, items } => {
                    self.workloads.insert(kind, items);
                }
                Event::PodDetail(detail) => self.pod_detail = Some(*detail),
                Event::ResourceYaml { key, yaml } => {
                    self.yaml_loaded_key = key;
                    self.yaml_text = yaml;
                }
                Event::Metrics { pods, nodes } => {
                    self.pod_metrics = pods
                        .into_iter()
                        .map(|m| (m.key, (m.cpu_millis, m.mem_bytes)))
                        .collect();
                    self.node_metrics = nodes
                        .into_iter()
                        .map(|m| (m.name, (m.cpu_millis, m.mem_bytes)))
                        .collect();
                }
                Event::PortForwardActive {
                    key,
                    namespace,
                    pod,
                    local_port,
                    target_port,
                } => {
                    self.forwards.push(ForwardInfo {
                        key,
                        namespace,
                        pod,
                        local_port,
                        target_port,
                    });
                }
                Event::PortForwardStopped { key, .. } => {
                    self.forwards.retain(|f| f.key != key);
                }
                Event::Notice(msg) => {
                    self.notice = Some(msg);
                    self.last_notice = Some(Instant::now());
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
            self.palette = if self.dark {
                theme::dark()
            } else {
                theme::light()
            };
        }
        // Re-détecte le thème système en mode « system ».
        if self.theme_mode == ThemeMode::Systeme {
            let sys_dark = dark_light::detect().is_ok_and(|m| m == dark_light::Mode::Dark);
            if sys_dark != self.dark {
                self.dark = sys_dark;
            }
        }
        theme::apply(ctx, &self.palette);

        self.poll_events();

        // Changement de page : démarre/arrête les flux du détail conteneur,
        // charge les ressources K8s à la demande.
        if self.prev_page.as_ref() != Some(&self.page) {
            if let Some(Page::ContainerDetail(name)) = &self.prev_page {
                self.send(naveo_core::Command::StopFollow {
                    key: format!("docker:{name}"),
                });
                self.send(naveo_core::Command::StopFollow {
                    key: format!("stats:{name}"),
                });
                self.following.remove(&format!("docker:{name}"));
            }
            match &self.page {
                Page::ContainerDetail(name) => {
                    let key = format!("docker:{name}");
                    self.following.insert(key.clone());
                    self.send(naveo_core::Command::FollowLogs { key });
                    self.send(naveo_core::Command::FollowStats(name.clone()));
                    self.send(naveo_core::Command::InspectContainer(name.clone()));
                }
                Page::ConfigMaps if !self.resources.contains_key("configmaps") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "configmaps".into(),
                    });
                }
                Page::Secrets if !self.resources.contains_key("secrets") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "secrets".into(),
                    });
                }
                Page::Ingress if !self.resources.contains_key("ingresses") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "ingresses".into(),
                    });
                }
                Page::Stockage if !self.resources.contains_key("pvcs") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "pvcs".into(),
                    });
                }
                Page::Endpoints if !self.resources.contains_key("endpoints") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "endpoints".into(),
                    });
                }
                Page::PVs if !self.resources.contains_key("pvs") => {
                    self.send(naveo_core::Command::LoadResources { kind: "pvs".into() });
                }
                Page::StorageClasses if !self.resources.contains_key("storageclasses") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "storageclasses".into(),
                    });
                }
                Page::ServiceAccounts if !self.resources.contains_key("serviceaccounts") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "serviceaccounts".into(),
                    });
                }
                Page::Roles if !self.resources.contains_key("roles") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "roles".into(),
                    });
                }
                Page::ClusterRoles if !self.resources.contains_key("clusterroles") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "clusterroles".into(),
                    });
                }
                Page::Bindings => {
                    if !self.resources.contains_key("rolebindings") {
                        self.send(naveo_core::Command::LoadResources {
                            kind: "rolebindings".into(),
                        });
                    }
                    if !self.resources.contains_key("clusterrolebindings") {
                        self.send(naveo_core::Command::LoadResources {
                            kind: "clusterrolebindings".into(),
                        });
                    }
                }
                Page::Namespaces if !self.resources.contains_key("namespaces") => {
                    self.send(naveo_core::Command::LoadResources {
                        kind: "namespaces".into(),
                    });
                }
                Page::StatefulSets => self.send(naveo_core::Command::LoadWorkloads {
                    kind: "statefulsets".into(),
                }),
                Page::DaemonSets => self.send(naveo_core::Command::LoadWorkloads {
                    kind: "daemonsets".into(),
                }),
                Page::ReplicaSets => self.send(naveo_core::Command::LoadWorkloads {
                    kind: "replicasets".into(),
                }),
                Page::Jobs => self.send(naveo_core::Command::LoadWorkloads {
                    kind: "jobs".into(),
                }),
                Page::CronJobs => self.send(naveo_core::Command::LoadWorkloads {
                    kind: "cronjobs".into(),
                }),
                Page::PodDetail { namespace, name } => {
                    self.pod_detail = None;
                    self.send(naveo_core::Command::LoadPodDetail {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    });
                    self.send(naveo_core::Command::LoadMetrics);
                }
                Page::YamlEditor {
                    kind,
                    namespace,
                    name,
                } => {
                    let key = format!("{kind}/{namespace}/{name}");
                    if self.yaml_loaded_key != key {
                        self.send(naveo_core::Command::GetResourceYaml {
                            kind: kind.clone(),
                            namespace: namespace.clone(),
                            name: name.clone(),
                        });
                    }
                }
                Page::YamlCreate if self.yaml_text.is_empty() => {
                    self.yaml_text = "# Colle ici le YAML de la ressource à créer.\n# apiVersion et metadata.name sont requis.\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: ma-config\n  namespace: default\ndata:\n  cle: valeur\n".into();
                }
                _ => {}
            }
            self.prev_page = Some(self.page.clone());
        }

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
            .frame(
                egui::Frame::default()
                    .fill(self.palette.bg)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| match self.page.clone() {
                Page::Conteneurs => views::containers::show(self, ui),
                Page::Images => views::images::show(self, ui),
                Page::Volumes => views::volumes::show(self, ui),
                Page::Reseaux => views::networks::show(self, ui),
                Page::Noeuds => views::k8s::nodes(self, ui),
                Page::Pods => views::k8s::pods(self, ui),
                Page::PodDetail { namespace, name } => {
                    views::k8s::pod_detail(self, ui, &namespace, &name)
                }
                Page::Services => views::k8s::services(self, ui),
                Page::Deploiements => views::k8s::deployments(self, ui),
                Page::StatefulSets => views::k8s::workloads(self, ui, "statefulsets"),
                Page::DaemonSets => views::k8s::workloads(self, ui, "daemonsets"),
                Page::ReplicaSets => views::k8s::workloads(self, ui, "replicasets"),
                Page::Jobs => views::k8s::workloads(self, ui, "jobs"),
                Page::CronJobs => views::k8s::workloads(self, ui, "cronjobs"),
                Page::Evenements => views::k8s::events(self, ui),
                Page::ConfigMaps => views::k8s::resources(self, ui, "configmaps"),
                Page::Secrets => views::k8s::resources(self, ui, "secrets"),
                Page::Ingress => views::k8s::resources(self, ui, "ingresses"),
                Page::Endpoints => views::k8s::resources(self, ui, "endpoints"),
                Page::Stockage => views::k8s::resources(self, ui, "pvcs"),
                Page::PVs => views::k8s::resources(self, ui, "pvs"),
                Page::StorageClasses => views::k8s::resources(self, ui, "storageclasses"),
                Page::ServiceAccounts => views::k8s::resources(self, ui, "serviceaccounts"),
                Page::Roles => views::k8s::resources(self, ui, "roles"),
                Page::ClusterRoles => views::k8s::resources(self, ui, "clusterroles"),
                Page::Bindings => views::k8s::bindings(self, ui),
                Page::Namespaces => views::k8s::resources(self, ui, "namespaces"),
                Page::YamlEditor {
                    kind,
                    namespace,
                    name,
                } => views::k8s::yaml_editor(self, ui, &kind, &namespace, &name),
                Page::YamlCreate => views::k8s::yaml_create(self, ui),
                Page::ContainerDetail(name) => views::detail::show(self, ui, &name),
                Page::Terminal => views::terminal::show(self, ui),
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

        // Toast de notification (succès d'une opération).
        if let (Some(msg), Some(when)) = (self.notice.clone(), self.last_notice) {
            if when.elapsed() < Duration::from_secs(4) {
                egui::Window::new("ℹ Naveo")
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -48.0])
                    .resizable(false)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.colored_label(self.palette.ok, &msg);
                    });
            } else {
                self.notice = None;
            }
        }
    }
}

impl NaveoApp {
    fn draw_sidebar(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        egui::SidePanel::left("sidebar")
            .exact_width(190.0)
            .frame(
                egui::Frame::default()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::same(10)),
            )
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
                ui.label(
                    egui::RichText::new("KUBERNETES")
                        .size(10.0)
                        .color(p.dim)
                        .strong(),
                );
                ui.add_space(2.0);

                // Groupes repliables (style Kiri) : workloads, réseau,
                // config, stockage, RBAC, cluster.
                egui::CollapsingHeader::new(
                    egui::RichText::new("⚡ Charge de travail")
                        .size(12.0)
                        .strong(),
                )
                .id_salt("nav-workloads")
                .default_open(true)
                .show(ui, |ui| {
                    self.nav_item(ui, "🫧", "Pods", Page::Pods);
                    self.nav_item(ui, "🚀", "Déploiements", Page::Deploiements);
                    self.nav_item(ui, "🔁", "StatefulSets", Page::StatefulSets);
                    self.nav_item(ui, "👾", "DaemonSets", Page::DaemonSets);
                    self.nav_item(ui, "🌱", "ReplicaSets", Page::ReplicaSets);
                    self.nav_item(ui, "📦", "Jobs", Page::Jobs);
                    self.nav_item(ui, "⏰", "CronJobs", Page::CronJobs);
                });
                egui::CollapsingHeader::new(egui::RichText::new("🌐 Réseau").size(12.0).strong())
                    .id_salt("nav-reseau")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.nav_item(ui, "🔌", "Services", Page::Services);
                        self.nav_item(ui, "🌍", "Ingress", Page::Ingress);
                        self.nav_item(ui, "🎯", "Endpoints", Page::Endpoints);
                    });
                egui::CollapsingHeader::new(
                    egui::RichText::new("🗂 Configuration").size(12.0).strong(),
                )
                .id_salt("nav-config")
                .default_open(true)
                .show(ui, |ui| {
                    self.nav_item(ui, "🗂", "ConfigMaps", Page::ConfigMaps);
                    self.nav_item(ui, "🔑", "Secrets", Page::Secrets);
                    self.nav_item(ui, "👤", "ServiceAccounts", Page::ServiceAccounts);
                });
                egui::CollapsingHeader::new(egui::RichText::new("🗄 Stockage").size(12.0).strong())
                    .id_salt("nav-stockage")
                    .default_open(false)
                    .show(ui, |ui| {
                        self.nav_item(ui, "💾", "PVC", Page::Stockage);
                        self.nav_item(ui, "📀", "Volumes (PV)", Page::PVs);
                        self.nav_item(ui, "🏷", "StorageClasses", Page::StorageClasses);
                    });
                egui::CollapsingHeader::new(egui::RichText::new("🛡 RBAC").size(12.0).strong())
                    .id_salt("nav-rbac")
                    .default_open(false)
                    .show(ui, |ui| {
                        self.nav_item(ui, "🗝", "Rôles", Page::Roles);
                        self.nav_item(ui, "👑", "ClusterRoles", Page::ClusterRoles);
                        self.nav_item(ui, "🔗", "Bindings", Page::Bindings);
                    });
                egui::CollapsingHeader::new(egui::RichText::new("🖥 Cluster").size(12.0).strong())
                    .id_salt("nav-cluster")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.nav_item(ui, "🖥", "Nœuds", Page::Noeuds);
                        self.nav_item(ui, "⚡", "Événements", Page::Evenements);
                        self.nav_item(ui, "🏷", "Namespaces", Page::Namespaces);
                    });
                ui.add_space(6.0);
                self.nav_item(ui, "📝", "Éditeur YAML", Page::YamlCreate);

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    ui.separator();
                    self.nav_item(ui, "⌨️", "Terminal", Page::Terminal);
                    self.nav_item(ui, "📜", "Journal", Page::Journal);
                    self.nav_item(ui, "⚙️", "Paramètres", Page::Parametres);
                });
            });
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, icon: &str, label: &str, page: Page) {
        let p = self.palette;
        let selected = self.page == page;
        let text = format!("{icon}  {label}");
        let resp = ui.selectable_label(
            selected,
            egui::RichText::new(&text)
                .size(14.0)
                .color(if selected { p.accent } else { p.text }),
        );
        if resp.clicked() {
            self.page = page;
        }
    }

    fn draw_topbar(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        egui::TopBottomPanel::top("topbar")
            .frame(
                egui::Frame::default()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Recherche
                    let search = ui.add_sized(
                        [280.0, 24.0],
                        egui::TextEdit::singleline(&mut self.search).hint_text("🔍  Filtrer…"),
                    );
                    if search.changed() {
                        // Le filtre est relu par les vues.
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("↻").clicked() {
                            self.refresh_visible();
                        }
                        if ui
                            .button("📁")
                            .on_hover_text("Ajouter un kubeconfig…")
                            .clicked()
                        {
                            self.import_kubeconfig();
                        }

                        // Sélecteur de cluster (multi-cluster).
                        if !self.contexts.is_empty() {
                            let current = if self.k8s_ok {
                                self.k8s_ctx.clone()
                            } else {
                                "Choisir un cluster…".to_string()
                            };
                            egui::ComboBox::from_id_salt("cluster-select")
                                .selected_text(egui::RichText::new(&current).size(12.0))
                                .width(230.0)
                                .show_ui(ui, |ui| {
                                    let mut picked: Option<(String, String)> = None;
                                    for ctx in &self.contexts {
                                        let selected = self.k8s_ok && self.k8s_ctx == ctx.name;
                                        if ui
                                            .selectable_label(
                                                selected,
                                                format!("{} · {}", ctx.name, ctx.cluster),
                                            )
                                            .clicked()
                                        {
                                            picked = Some((ctx.file.clone(), ctx.name.clone()));
                                        }
                                    }
                                    if let Some((file, name)) = picked {
                                        self.send(naveo_core::Command::ConnectContext {
                                            file,
                                            context: name.clone(),
                                        });
                                        self.k8s_ctx = name;
                                    }
                                });
                        }

                        status_chip(ui, p, "Kubernetes", self.k8s_ok);
                        status_chip(ui, p, "Docker", self.docker_ok);
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

/// Type de workload associé à une page (pour le rafraîchissement auto).
fn workload_page_kind(page: &Page) -> Option<&'static str> {
    match page {
        Page::StatefulSets => Some("statefulsets"),
        Page::DaemonSets => Some("daemonsets"),
        Page::ReplicaSets => Some("replicasets"),
        Page::Jobs => Some("jobs"),
        Page::CronJobs => Some("cronjobs"),
        _ => None,
    }
}

/// Types de ressources en lecture associés à une page.
/// (« Bindings » regroupe rolebindings + clusterrolebindings.)
fn resource_page_kind(page: &Page) -> Option<&'static [&'static str]> {
    match page {
        Page::ConfigMaps => Some(&["configmaps"]),
        Page::Secrets => Some(&["secrets"]),
        Page::Ingress => Some(&["ingresses"]),
        Page::Endpoints => Some(&["endpoints"]),
        Page::Stockage => Some(&["pvcs"]),
        Page::PVs => Some(&["pvs"]),
        Page::StorageClasses => Some(&["storageclasses"]),
        Page::ServiceAccounts => Some(&["serviceaccounts"]),
        Page::Roles => Some(&["roles"]),
        Page::ClusterRoles => Some(&["clusterroles"]),
        Page::Bindings => Some(&["rolebindings", "clusterrolebindings"]),
        Page::Namespaces => Some(&["namespaces"]),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Préférences persistées (~/.naveo/prefs.json)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Prefs {
    /// dark | light | system
    #[serde(default)]
    theme: String,
    #[serde(default)]
    log_timestamps: bool,
}

fn prefs_path() -> Option<std::path::PathBuf> {
    home::home_dir().map(|h| h.join(".naveo").join("prefs.json"))
}

fn load_prefs() -> Prefs {
    let Some(path) = prefs_path() else {
        return Prefs {
            theme: "dark".into(),
            log_timestamps: false,
        };
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Prefs {
            theme: "dark".into(),
            log_timestamps: false,
        })
}

fn prefs_of(app: &NaveoApp) -> Prefs {
    Prefs {
        theme: app.theme_mode.key().into(),
        log_timestamps: app.log_timestamps,
    }
}

fn save_prefs(prefs: &Prefs) {
    if let Some(path) = prefs_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(prefs) {
            let _ = std::fs::write(path, json);
        }
    }
}
