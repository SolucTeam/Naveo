//! Client Kubernetes (via kube-rs) : nœuds, pods, déploiements, logs.

use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Node, Pod, Service};
use kube::api::{AttachParams, DeleteParams, ListParams, LogParams};
use kube::{Api, Client};

use crate::events::{DeploymentInfo, Event, NodeInfo, PodInfo, ServiceInfo};

/// Se connecte au cluster du contexte par défaut (KUBECONFIG / ~/.kube/config)
/// et renvoie le client ainsi que le nom du contexte.
pub async fn connect() -> Result<(Client, String)> {
    let client = Client::try_default().await?;
    let context = kube::config::Kubeconfig::read()
        .ok()
        .and_then(|cfg| cfg.current_context.clone())
        .unwrap_or_else(|| "default".into());
    Ok((client, context))
}

// ---------------------------------------------------------------------------
// Multi-cluster
// ---------------------------------------------------------------------------

/// Découvre tous les contextes disponibles : `$KUBECONFIG` (séparé par « : »),
/// sinon tous les fichiers de `~/.kube/` (un fichier par environnement).
pub fn list_contexts() -> Vec<crate::events::ContextInfo> {
    use crate::events::ContextInfo;
    use std::path::PathBuf;

    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(kc) = std::env::var("KUBECONFIG") {
        for p in kc.split(':').filter(|s| !s.is_empty()) {
            paths.push(PathBuf::from(p));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        let dir = PathBuf::from(home).join(".kube");
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() {
                    paths.push(p);
                }
            }
        }
    }

    let mut out: Vec<ContextInfo> = Vec::new();
    for path in paths {
        // Ignore les fichiers temporaires et les sauvegardes expirées.
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.starts_with('.') || name.contains("expired") {
            continue;
        }

        let Ok(cfg) = kube::config::Kubeconfig::read_from(&path) else {
            continue;
        };
        let file = path.display().to_string();
        for named in cfg.contexts {
            let cluster = named
                .context
                .as_ref()
                .map(|c| c.cluster.clone())
                .unwrap_or_default();
            out.push(ContextInfo {
                file: file.clone(),
                name: named.name,
                cluster,
            });
        }
    }

    out.sort();
    out.dedup();
    out
}

/// Se connecte à un contexte précis d'un kubeconfig donné.
pub async fn connect_to(file: &str, context: &str) -> Result<(Client, String)> {
    let mut cfg = kube::config::Kubeconfig::read_from(std::path::Path::new(file))?;
    cfg.current_context = Some(context.to_string());

    let config = kube::Config::from_custom_kubeconfig(cfg, &Default::default()).await?;
    let client = Client::try_from(config)?;
    Ok((client, context.to_string()))
}

// ---------------------------------------------------------------------------
// Nœuds
// ---------------------------------------------------------------------------

pub async fn list_nodes(client: &Client) -> Result<Vec<NodeInfo>> {
    let nodes: Vec<Node> = Api::all(client.clone())
        .list(&ListParams::default())
        .await?
        .items;

    Ok(nodes
        .into_iter()
        .map(|n| {
            let status = n.status.unwrap_or_default();
            let ready = status
                .conditions
                .as_ref()
                .and_then(|cs| {
                    cs.iter()
                        .find(|c| c.type_ == "Ready")
                        .map(|c| c.status == "True")
                })
                .unwrap_or(false);
            let roles = n
                .metadata
                .labels
                .unwrap_or_default()
                .keys()
                .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let internal_ip = status
                .addresses
                .unwrap_or_default()
                .iter()
                .find(|a| a.type_ == "InternalIP")
                .map(|a| a.address.clone())
                .unwrap_or_default();
            let allocatable = status.allocatable.unwrap_or_default();

            NodeInfo {
                name: n.metadata.name.unwrap_or_default(),
                ready,
                kubelet_version: status
                    .node_info
                    .map(|ni| ni.kubelet_version)
                    .unwrap_or_default(),
                internal_ip,
                roles,
                cpu: allocatable
                    .get("cpu")
                    .map(|q| q.0.clone())
                    .unwrap_or_default(),
                memory: allocatable
                    .get("memory")
                    .map(|q| q.0.clone())
                    .unwrap_or_default(),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Pods
// ---------------------------------------------------------------------------

pub async fn list_pods(client: &Client) -> Result<Vec<PodInfo>> {
    let pods: Vec<Pod> = Api::all(client.clone())
        .list(&ListParams::default())
        .await?
        .items;

    Ok(pods
        .into_iter()
        .map(|p| {
            let meta = p.metadata;
            let spec = p.spec.unwrap_or_default();
            let status = p.status.unwrap_or_default();

            // État par conteneur (spec ∩ status.containerStatuses).
            let statuses = status.container_statuses.clone().unwrap_or_default();
            let containers = spec
                .containers
                .iter()
                .map(|c| {
                    let s = statuses.iter().find(|s| s.name == c.name);
                    let (state, reason) = match s.and_then(|s| s.state.as_ref()) {
                        Some(st) if st.running.is_some() => ("running".to_string(), String::new()),
                        Some(st) if st.terminated.is_some() => {
                            let t = st.terminated.as_ref().expect("terminé");
                            (
                                "terminated".to_string(),
                                t.reason.clone().unwrap_or_default(),
                            )
                        }
                        Some(st) if st.waiting.is_some() => {
                            let w = st.waiting.as_ref().expect("en attente");
                            ("waiting".to_string(), w.reason.clone().unwrap_or_default())
                        }
                        _ => ("waiting".to_string(), String::new()),
                    };

                    crate::events::ContainerStatusInfo {
                        name: c.name.clone(),
                        image: c.image.clone().unwrap_or_default(),
                        ready: s.map(|s| s.ready).unwrap_or(false),
                        restarts: s.map(|s| s.restart_count).unwrap_or(0),
                        state,
                        reason,
                    }
                })
                .collect();
            let restarts: i32 = statuses.iter().map(|cs| cs.restart_count).sum();
            let owner = meta
                .owner_references
                .unwrap_or_default()
                .first()
                .map(|o| o.kind.clone())
                .unwrap_or_default();

            PodInfo {
                name: meta.name.unwrap_or_default(),
                namespace: meta.namespace.unwrap_or_default(),
                node: spec.node_name.unwrap_or_default(),
                pod_ip: status.pod_ip.unwrap_or_default(),
                phase: status.phase.unwrap_or_else(|| "Unknown".into()),
                restarts,
                containers,
                owner,
            }
        })
        .collect())
}

pub async fn delete_pod(client: &Client, namespace: &str, name: &str) -> Result<()> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    pods.delete(name, &DeleteParams::default()).await?;
    Ok(())
}

pub async fn pod_logs(
    client: &Client,
    namespace: &str,
    name: &str,
    container: Option<&str>,
    previous: bool,
) -> Result<String> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let params = LogParams {
        container: container.filter(|c| !c.is_empty()).map(|c| c.to_string()),
        tail_lines: Some(500),
        previous,
        ..Default::default()
    };
    Ok(pods.logs(name, &params).await?)
}

/// Suit les logs d'un pod en direct et pousse chaque ligne sur
/// le canal d'événements (clé de flux passée telle quelle).
pub async fn stream_pod_logs(
    client: &Client,
    namespace: &str,
    name: &str,
    container: Option<&str>,
    key: &str,
    tx: crossbeam_channel::Sender<Event>,
) -> Result<()> {
    use futures::io::AsyncBufReadExt;

    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let params = LogParams {
        container: container.filter(|c| !c.is_empty()).map(|c| c.to_string()),
        tail_lines: Some(200),
        follow: true,
        ..Default::default()
    };

    let mut reader = pods.log_stream(name, &params).await?;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if tx
                    .send(Event::LogChunk {
                        target: key.into(),
                        text: line,
                    })
                    .is_err()
                {
                    break; // l'UI est fermée
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Exec (terminal)
// ---------------------------------------------------------------------------

/// Exécute `sh -c <cmd>` dans le pod (1er conteneur si non précisé)
/// et renvoie la sortie combinée stdout + stderr.
pub async fn pod_exec(
    client: &Client,
    namespace: &str,
    name: &str,
    container: Option<&str>,
    cmd: &str,
) -> Result<String> {
    use tokio::io::AsyncReadExt;

    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let params = AttachParams {
        stdin: false,
        stdout: true,
        stderr: true,
        container: container.filter(|c| !c.is_empty()).map(|c| c.to_string()),
        ..Default::default()
    };

    let mut proc = pods
        .exec(
            name,
            vec!["sh".to_string(), "-c".to_string(), cmd.to_string()],
            &params,
        )
        .await?;

    let mut out = String::new();
    if let Some(mut stdout) = proc.stdout() {
        let mut buf = vec![0u8; 65_536];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    }

    // Attend la fin du process : en cas d'échec, K8s renvoie un Status
    // (« Failure », raison + message). On l'affiche de façon lisible.
    if let Some(status) = proc.take_status() {
        if let Some(status) = status.await {
            if status.status.as_deref() == Some("Failure") {
                let reason = status.reason.clone().unwrap_or_default();
                let message = status.message.clone().unwrap_or_default();
                out.push_str(&format!("\n[échec : {reason} {message}]"));
            }
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Déploiements
// ---------------------------------------------------------------------------

pub async fn list_deployments(client: &Client) -> Result<Vec<DeploymentInfo>> {
    let deps: Vec<Deployment> = Api::all(client.clone())
        .list(&ListParams::default())
        .await?
        .items;

    Ok(deps
        .into_iter()
        .map(|d| {
            let status = d.status.unwrap_or_default();
            let image = d
                .spec
                .as_ref()
                .and_then(|s| s.template.spec.as_ref())
                .and_then(|ps| ps.containers.first())
                .map(|c| c.image.clone().unwrap_or_default())
                .unwrap_or_default();

            DeploymentInfo {
                name: d.metadata.name.unwrap_or_default(),
                namespace: d.metadata.namespace.unwrap_or_default(),
                replicas_ready: status.ready_replicas.unwrap_or(0),
                replicas_desired: status.replicas.unwrap_or(0),
                image,
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

pub async fn list_services(client: &Client) -> Result<Vec<ServiceInfo>> {
    let services: Vec<Service> = Api::all(client.clone())
        .list(&ListParams::default())
        .await?
        .items;

    Ok(services
        .into_iter()
        .map(|s| {
            let spec = s.spec.unwrap_or_default();
            let ports = spec
                .ports
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let target = match &p.target_port {
                        Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i)) => {
                            i.to_string()
                        }
                        Some(
                            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s),
                        ) => s.clone(),
                        None => String::new(),
                    };
                    match p.node_port {
                        Some(np) => format!("{}:{}→{}", p.port, np, target),
                        None => format!("{}→{}", p.port, target),
                    }
                })
                .collect();

            ServiceInfo {
                name: s.metadata.name.unwrap_or_default(),
                namespace: s.metadata.namespace.unwrap_or_default(),
                service_type: spec.type_.unwrap_or_else(|| "ClusterIP".into()),
                cluster_ip: spec.cluster_ip.unwrap_or_default(),
                ports,
            }
        })
        .collect())
}
