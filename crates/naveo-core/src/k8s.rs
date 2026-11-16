//! Client Kubernetes (via kube-rs) : nœuds, pods, déploiements, logs.

use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Node, Pod, Service};
use kube::api::{AttachParams, DeleteParams, ListParams, LogParams};
use kube::{Api, Client};

use crate::events::{
    DeploymentInfo, Event, K8sEventInfo, NodeInfo, PodInfo, ResourceInfo, ServiceInfo,
};

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

// ---------------------------------------------------------------------------
// Événements
// ---------------------------------------------------------------------------

/// Âge lisible depuis une durée en secondes.
fn human_age(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}j", secs / 86400)
    }
}

/// Charge les événements récents du cluster (triés du plus récent).
pub async fn list_events(client: &Client) -> Result<Vec<K8sEventInfo>> {
    use chrono::Utc;
    use k8s_openapi::api::core::v1::Event as K8sEvent;

    let events: Vec<K8sEvent> = Api::all(client.clone())
        .list(&ListParams::default())
        .await?
        .items;

    let now = Utc::now();
    // (timestamp, warning, info) — trié : warnings d'abord, plus récents ensuite.
    let mut out: Vec<(Option<chrono::DateTime<chrono::Utc>>, bool, K8sEventInfo)> = events
        .into_iter()
        .map(|e| {
            let meta = &e.metadata;
            let involved = &e.involved_object;
            // k8s-openapi 0.25 : Time.0 est déjà un DateTime<Utc>.
            let ts = e
                .last_timestamp
                .as_ref()
                .map(|t| t.0)
                .or_else(|| e.event_time.as_ref().map(|t| t.0));
            let last_seen = ts
                .map(|t| human_age((now - t).num_seconds().max(0)))
                .unwrap_or_else(|| "—".into());
            let warning = e.type_.as_deref() == Some("Warning");

            let info = K8sEventInfo {
                namespace: meta.namespace.clone().unwrap_or_default(),
                involved: format!(
                    "{}/{}",
                    involved.kind.clone().unwrap_or_default(),
                    involved.name.clone().unwrap_or_default()
                ),
                event_type: e.type_.clone().unwrap_or_default(),
                reason: e.reason.clone().unwrap_or_default(),
                message: e.message.clone().unwrap_or_default(),
                count: e.count.unwrap_or(1),
                last_seen,
            };
            (ts, warning, info)
        })
        .collect();

    out.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    Ok(out.into_iter().map(|(_, _, info)| info).collect())
}

// ---------------------------------------------------------------------------
// Déploiements : scale / rollout restart
// ---------------------------------------------------------------------------

/// Change le nombre de réplicas d'un déploiement.
pub async fn scale_deployment(
    client: &Client,
    namespace: &str,
    name: &str,
    replicas: i32,
) -> Result<()> {
    use kube::api::Patch;
    use kube::api::PatchParams;

    let deploys: Api<Deployment> = Api::namespaced(client.clone(), namespace);
    let patch = serde_json::json!({ "spec": { "replicas": replicas } });
    deploys
        .patch(name, &PatchParams::default(), &Patch::Strategic(&patch))
        .await?;
    Ok(())
}

/// Rollout restart : annotation restartedAt sur le template des pods.
pub async fn rollout_restart(client: &Client, namespace: &str, name: &str) -> Result<()> {
    use kube::api::Patch;
    use kube::api::PatchParams;

    let deploys: Api<Deployment> = Api::namespaced(client.clone(), namespace);
    let now = chrono::Utc::now().to_rfc3339();
    let patch = serde_json::json!({
        "spec": {
            "template": {
                "metadata": {
                    "annotations": {
                        "kubectl.kubernetes.io/restartedAt": now
                    }
                }
            }
        }
    });
    deploys
        .patch(name, &PatchParams::default(), &Patch::Strategic(&patch))
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ressources génériques en lecture (configmaps, secrets, ingresses, pvcs)
// ---------------------------------------------------------------------------

/// Charge une liste de ressources : configmaps|secrets|ingresses|pvcs.
pub async fn load_resources(client: &Client, kind: &str) -> Result<Vec<ResourceInfo>> {
    use chrono::{DateTime, Utc};
    use k8s_openapi::api::core::v1::{ConfigMap, PersistentVolumeClaim, Secret};
    use k8s_openapi::api::networking::v1::Ingress;
    use kube::api::ObjectMeta;

    fn age_of(meta: &ObjectMeta, now: DateTime<Utc>) -> String {
        meta.creation_timestamp
            .as_ref()
            .map(|t| human_age((now - t.0).num_seconds().max(0)))
            .unwrap_or_else(|| "—".into())
    }

    let now = Utc::now();
    let items = match kind {
        "configmaps" => {
            let list: Vec<ConfigMap> = Api::all(client.clone())
                .list(&ListParams::default())
                .await?
                .items;
            list.into_iter()
                .map(|cm| {
                    let keys = cm
                        .data
                        .unwrap_or_default()
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    ResourceInfo {
                        name: cm.metadata.name.clone().unwrap_or_default(),
                        namespace: cm.metadata.namespace.clone().unwrap_or_default(),
                        summary: if keys.is_empty() { "—".into() } else { keys },
                        age: age_of(&cm.metadata, now),
                    }
                })
                .collect()
        }
        "secrets" => {
            let list: Vec<Secret> = Api::all(client.clone())
                .list(&ListParams::default())
                .await?
                .items;
            list.into_iter()
                .map(|s| {
                    let keys = s
                        .data
                        .unwrap_or_default()
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    ResourceInfo {
                        name: s.metadata.name.clone().unwrap_or_default(),
                        namespace: s.metadata.namespace.clone().unwrap_or_default(),
                        summary: if keys.is_empty() { "—".into() } else { keys },
                        age: age_of(&s.metadata, now),
                    }
                })
                .collect()
        }
        "ingresses" => {
            let list: Vec<Ingress> = Api::all(client.clone())
                .list(&ListParams::default())
                .await?
                .items;
            list.into_iter()
                .map(|i| {
                    let hosts: Vec<String> = i
                        .spec
                        .as_ref()
                        .map(|s| {
                            s.rules
                                .iter()
                                .flatten()
                                .filter_map(|r| r.host.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    ResourceInfo {
                        name: i.metadata.name.clone().unwrap_or_default(),
                        namespace: i.metadata.namespace.clone().unwrap_or_default(),
                        summary: if hosts.is_empty() {
                            "—".into()
                        } else {
                            hosts.join(", ")
                        },
                        age: age_of(&i.metadata, now),
                    }
                })
                .collect()
        }
        "pvcs" => {
            let list: Vec<PersistentVolumeClaim> = Api::all(client.clone())
                .list(&ListParams::default())
                .await?
                .items;
            list.into_iter()
                .map(|p| {
                    let phase = p
                        .status
                        .as_ref()
                        .and_then(|s| s.phase.clone())
                        .unwrap_or_default();
                    let storage = p
                        .spec
                        .as_ref()
                        .and_then(|s| s.resources.as_ref())
                        .and_then(|r| r.requests.as_ref())
                        .and_then(|r| r.get("storage"))
                        .map(|q| q.0.clone())
                        .unwrap_or_default();
                    ResourceInfo {
                        name: p.metadata.name.clone().unwrap_or_default(),
                        namespace: p.metadata.namespace.clone().unwrap_or_default(),
                        summary: format!("{phase} · {storage}"),
                        age: age_of(&p.metadata, now),
                    }
                })
                .collect()
        }
        _ => anyhow::bail!("type de ressource inconnu : {kind}"),
    };
    Ok(items)
}

// ---------------------------------------------------------------------------
// Port-forward
// ---------------------------------------------------------------------------

/// Redirige un port local vers un pod : accepte les connexions TCP sur
/// `local_port` et les relaye via l'API du cluster. Chaque connexion
/// ouvre son propre canal de forwarding.
pub async fn port_forward(
    client: &Client,
    namespace: &str,
    pod: &str,
    target_port: u16,
    local_port: u16,
    key: String,
    tx: crossbeam_channel::Sender<Event>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", local_port)).await?;
    let _ = tx.send(Event::PortForwardActive {
        key: key.clone(),
        namespace: namespace.into(),
        pod: pod.into(),
        local_port,
        target_port,
    });

    loop {
        let (mut sock, _peer) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => break,
        };
        let client = client.clone();
        let ns = namespace.to_string();
        let pod_name = pod.to_string();
        let tx2 = tx.clone();
        let key2 = key.clone();
        tokio::spawn(async move {
            let pods: Api<Pod> = Api::namespaced(client, &ns);
            let mut forwarder = match pods.portforward(&pod_name, &[target_port]).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx2.send(Event::PortForwardStopped {
                        key: key2,
                        reason: e.to_string(),
                    });
                    return;
                }
            };
            let mut upstream = match forwarder.take_stream(target_port) {
                Some(s) => s,
                None => {
                    let _ = tx2.send(Event::PortForwardStopped {
                        key: key2,
                        reason: format!("port {target_port} absent du forwarding"),
                    });
                    return;
                }
            };
            let _ = tokio::io::copy_bidirectional(&mut sock, &mut upstream).await;
            drop(upstream);
            let _ = forwarder.join().await;
        });
    }
    Ok(())
}
