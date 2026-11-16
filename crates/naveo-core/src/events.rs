//! Types échangés entre le backend asynchrone et l'UI.

/// Une action demandée par l'UI, exécutée en arrière-plan.
#[derive(Debug, Clone)]
pub enum Command {
    // Docker
    RefreshContainers,
    RefreshImages,
    RefreshVolumes,
    RefreshNetworks,
    StartContainer(String),
    StopContainer(String),
    RestartContainer(String),
    PauseContainer(String),
    UnpauseContainer(String),
    KillContainer(String),
    RemoveContainer(String),
    RemoveImage(String),
    /// Récupère des logs one-shot (avec options d'affichage).
    ContainerLogs {
        name: String,
        /// Préfixer chaque ligne d'un timestamp.
        timestamps: bool,
        /// Nombre de lignes à récupérer (0 = tout).
        tail: i64,
    },
    /// Action sur tout un projet Compose : start | stop | restart | remove.
    ProjectAction {
        project: String,
        action: String,
    },

    // Détail conteneur
    /// JSON d'inspection du conteneur (one-shot).
    InspectContainer(String),
    /// Suit les logs en direct (clé : « docker:<name> »).
    FollowLogs {
        key: String,
    },
    /// Suit les stats en direct (clé : « stats:<name> »).
    FollowStats(String),
    /// Arrête un flux précédemment lancé (logs ou stats).
    StopFollow {
        key: String,
    },

    // Actions en masse / prune
    /// Applique une action (start|stop|restart|remove|pause) à plusieurs conteneurs.
    BulkContainers {
        names: Vec<String>,
        action: String,
    },
    /// Nettoie les ressources inutilisées : containers|images|volumes|networks.
    Prune {
        kind: String,
    },

    // Images : pull, Docker Hub, gestion avancée
    /// Recherche des images sur Docker Hub.
    HubSearch {
        query: String,
    },
    /// Liste les tags d'un dépôt Docker Hub.
    HubTags {
        repo: String,
    },
    /// Tire une image (ex. « nginx:alpine »).
    PullImage {
        image: String,
    },
    /// Crée et démarre un conteneur à partir d'une image.
    RunContainer {
        image: String,
        name: String,
        env: Vec<(String, String)>,
        /// (port conteneur, port hôte)
        ports: Vec<(String, String)>,
    },
    /// Applique un nouveau tag à une image.
    TagImage {
        image: String,
        tag: String,
    },
    /// Exporte une image vers un fichier tar.
    SaveImage {
        image: String,
        path: String,
    },
    /// Importe une image depuis un fichier tar.
    LoadImage {
        path: String,
    },
    /// Historique des couches d'une image.
    ImageHistory(String),
    /// Connecte un registre (écrit ~/.docker/config.json).
    RegistryLogin {
        server: String,
        username: String,
        password: String,
    },
    /// Déconnecte un registre.
    RegistryLogout(String),

    // Fichiers du conteneur
    /// Liste un répertoire du conteneur (via archive tar).
    ListFiles {
        name: String,
        path: String,
    },
    /// Aperçu d'un fichier du conteneur.
    PreviewFile {
        name: String,
        path: String,
    },
    /// Sauvegarde un fichier/répertoire du conteneur vers le disque.
    SaveFile {
        name: String,
        path: String,
        dest: String,
    },

    // Édition à chaud
    /// Renomme un conteneur.
    RenameContainer {
        name: String,
        new_name: String,
    },
    /// Crée un commit (image) à partir d'un conteneur.
    CommitContainer {
        name: String,
        repo: String,
        tag: String,
    },
    /// Exporte le système de fichiers du conteneur en tar.
    ExportContainer {
        name: String,
        path: String,
    },
    /// Recrée le conteneur avec de nouvelles variables d'env / ports.
    RecreateContainer {
        name: String,
        env: Vec<(String, String)>,
        /// (port conteneur, port hôte)
        ports: Vec<(String, String)>,
    },

    // Kubernetes
    RefreshK8s,
    DeletePod {
        namespace: String,
        name: String,
    },
    PodLogs {
        namespace: String,
        name: String,
        container: String,
        previous: bool,
    },
    /// Passe un déploiement à `replicas` réplicas.
    ScaleDeployment {
        namespace: String,
        name: String,
        replicas: i32,
    },
    /// Redémarre un déploiement (rollout restart, annotation restartedAt).
    RolloutRestart {
        namespace: String,
        name: String,
    },
    /// Charge un type de ressource en lecture : configmaps|secrets|ingresses|pvcs.
    LoadResources {
        kind: String,
    },
    /// Redirige un port local vers un pod (clé : « pf:<local_port> »).
    PortForward {
        namespace: String,
        pod: String,
        target_port: u16,
        local_port: u16,
    },
    /// Arrête un port-forward (clé : « pf:<local_port> »).
    StopPortForward {
        key: String,
    },

    // Exécution de commandes (terminal)
    /// Lance une commande dans un conteneur Docker (`sh -c <cmd>`).
    ExecContainer {
        name: String,
        cmd: String,
    },
    /// Lance une commande dans un pod Kubernetes (`sh -c <cmd>`).
    PodExec {
        namespace: String,
        name: String,
        container: String,
        cmd: String,
    },

    // Multi-cluster
    /// Se connecte au contexte `context` défini dans le fichier `file`.
    ConnectContext {
        file: String,
        context: String,
    },
    /// Rescanne les kubeconfigs locaux et renvoie ContextsAvailable.
    RefreshContexts,
}

/// Un état ou un résultat poussé vers l'UI.
#[derive(Debug, Clone)]
pub enum Event {
    Connected {
        docker: bool,
        kubernetes: bool,
        context: String,
    },
    Containers(Vec<ContainerInfo>),
    Images(Vec<ImageInfo>),
    Volumes(Vec<VolumeInfo>),
    Networks(Vec<NetworkInfo>),
    Nodes(Vec<NodeInfo>),
    Pods(Vec<PodInfo>),
    Deployments(Vec<DeploymentInfo>),
    Services(Vec<ServiceInfo>),
    Logs {
        target: String,
        text: String,
    },
    /// Morceau de log en direct (stream). `target` est la clé du flux.
    LogChunk {
        target: String,
        text: String,
    },
    /// Résultat d'une commande exécutée (terminal). `text` est ajouté
    /// au tampon du terminal `target`.
    ExecOutput {
        target: String,
        text: String,
    },
    /// JSON d'inspection d'un conteneur.
    Inspect {
        target: String,
        json: String,
    },
    /// Relevé de ressources d'un conteneur.
    Stats {
        name: String,
        info: StatsInfo,
    },
    /// Contextes Kubernetes découverts dans les kubeconfigs locaux.
    ContextsAvailable(Vec<ContextInfo>),
    /// Connexion réussie à un contexte Kubernetes.
    K8sConnected {
        context: String,
    },
    /// Résultats d'une recherche Docker Hub.
    HubResults(Vec<HubRepo>),
    /// Tags disponibles pour un dépôt Docker Hub.
    HubTags {
        repo: String,
        tags: Vec<String>,
    },
    /// Historique (couches) d'une image.
    ImageHistory {
        image: String,
        layers: Vec<ImageLayerInfo>,
    },
    /// Contenu d'un répertoire du conteneur.
    FileList {
        name: String,
        path: String,
        entries: Vec<ContainerFile>,
    },
    /// Aperçu d'un fichier du conteneur.
    FilePreview {
        name: String,
        path: String,
        content: String,
    },
    /// Événements Kubernetes récents.
    Events(Vec<K8sEventInfo>),
    /// Liste de ressources en lecture (configmaps, secrets…).
    Resources {
        kind: String,
        items: Vec<ResourceInfo>,
    },
    /// Un port-forward est actif.
    PortForwardActive {
        key: String,
        namespace: String,
        pod: String,
        local_port: u16,
        target_port: u16,
    },
    /// Un port-forward s'est arrêté (fin de flux ou erreur).
    PortForwardStopped {
        key: String,
        reason: String,
    },
    /// Notification générique (succès d'une opération…).
    Notice(String),
    Error(String),
}

/// Contextes Kubernetes découverts dans les kubeconfigs locaux.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextInfo {
    /// Chemin du fichier kubeconfig.
    pub file: String,
    /// Nom du contexte.
    pub name: String,
    /// Nom du cluster pointé.
    pub cluster: String,
}

// ---------------------------------------------------------------------------
// Modèles Docker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    /// running, exited, paused, created, restarting…
    pub state: String,
    /// Statut lisible (« Up 2 minutes », « Exited (0) 3 days ago »…)
    pub status: String,
    pub ports: Vec<String>,
    /// Projet Compose (`com.docker.compose.project`), s'il existe.
    pub project: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ImageInfo {
    pub tags: Vec<String>,
    pub size_bytes: u64,
    pub created: i64,
    pub id: String,
}

#[derive(Debug, Clone, Default)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkInfo {
    pub name: String,
    pub driver: String,
    pub scope: String,
}

/// Relevé de ressources instantané d'un conteneur.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatsInfo {
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_limit_bytes: u64,
    pub mem_percent: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

// ---------------------------------------------------------------------------
// Modèles Kubernetes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct NodeInfo {
    pub name: String,
    pub ready: bool,
    pub kubelet_version: String,
    pub internal_ip: String,
    pub roles: String,
    pub cpu: String,
    pub memory: String,
}

/// État d'un conteneur au sein d'un pod.
#[derive(Debug, Clone, Default)]
pub struct ContainerStatusInfo {
    pub name: String,
    pub image: String,
    pub ready: bool,
    pub restarts: i32,
    /// running | waiting | terminated
    pub state: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct PodInfo {
    pub name: String,
    pub namespace: String,
    /// Nœud sur lequel tourne le pod (spec.nodeName).
    pub node: String,
    pub pod_ip: String,
    /// Phase du pod : Running, Pending, Succeeded, Failed…
    pub phase: String,
    pub restarts: i32,
    pub containers: Vec<ContainerStatusInfo>,
    /// Kind du propriétaire (Deployment, DaemonSet, Job…), s'il existe.
    pub owner: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub replicas_ready: i32,
    pub replicas_desired: i32,
    pub image: String,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceInfo {
    pub name: String,
    pub namespace: String,
    /// ClusterIP, NodePort, LoadBalancer, ExternalName…
    pub service_type: String,
    pub cluster_ip: String,
    pub ports: Vec<String>,
}

// ---------------------------------------------------------------------------
// Modèles Docker Hub, fichiers, couches, K8s génériques
// ---------------------------------------------------------------------------

/// Dépôt trouvé sur Docker Hub.
#[derive(Debug, Clone, Default)]
pub struct HubRepo {
    pub name: String,
    pub description: String,
    pub stars: u64,
    pub pulls: u64,
    pub official: bool,
}

/// Une couche de l'historique d'une image.
#[derive(Debug, Clone, Default)]
pub struct ImageLayerInfo {
    /// « CreatedBy » (commande de la couche, souvent une commande shell).
    pub created_by: String,
    pub size_bytes: u64,
    /// Secondes depuis l'époque.
    pub created: i64,
}

/// Une entrée dans un répertoire du conteneur.
#[derive(Debug, Clone, Default)]
pub struct ContainerFile {
    pub name: String,
    /// Octets (0 pour un répertoire).
    pub size: u64,
    pub is_dir: bool,
    /// Permissions lisibles (« drwxr-xr-x ») si disponibles.
    pub mode: String,
}

/// Événement Kubernetes (type Event du cluster).
#[derive(Debug, Clone, Default)]
pub struct K8sEventInfo {
    pub namespace: String,
    /// Objet concerné (« Pod/nginx-abc »).
    pub involved: String,
    /// Normal | Warning
    pub event_type: String,
    pub reason: String,
    pub message: String,
    pub count: i32,
    /// Dernière occurrence (« 3 min », « 2 h »…).
    pub last_seen: String,
}

/// Ressource Kubernetes générique en lecture seule.
#[derive(Debug, Clone, Default)]
pub struct ResourceInfo {
    pub name: String,
    pub namespace: String,
    /// Détails courts selon le type (clés d'une ConfigMap, type/endpoints…).
    pub summary: String,
    /// Âge lisible (« 3 d », « 2 h »…).
    pub age: String,
}
