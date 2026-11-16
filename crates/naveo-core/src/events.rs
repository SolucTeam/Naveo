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
    RemoveContainer(String),
    RemoveImage(String),
    ContainerLogs(String),

    // Kubernetes
    RefreshK8s,
    DeletePod { namespace: String, name: String },
    PodLogs { namespace: String, name: String, container: String },
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
    Logs {
        target: String,
        text: String,
    },
    Error(String),
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

#[derive(Debug, Clone, Default)]
pub struct PodInfo {
    pub name: String,
    pub namespace: String,
    pub node: String,
    /// Phase du pod : Running, Pending, Succeeded, Failed…
    pub phase: String,
    pub restarts: i32,
    pub containers: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub replicas_ready: i32,
    pub replicas_desired: i32,
    pub image: String,
}
