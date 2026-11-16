//! Client Kubernetes (via kube-rs) : nœuds, pods, déploiements, logs.

use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Node, Pod, Service};
use kube::api::{AttachParams, DeleteParams, ListParams, LogParams};
use kube::{Api, Client};

use crate::events::{
    DeploymentInfo, Event, K8sEventInfo, NodeInfo, PodInfo, ResourceInfo, ServiceInfo, WorkloadInfo,
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

/// Charge une liste de ressources en lecture : configmaps, secrets, ingresses,
/// pvcs, endpoints, pvs, storageclasses, serviceaccounts, roles, clusterroles,
/// rolebindings, clusterrolebindings, hpas, pdbs, networkpolicies,
/// resourcequotas, namespaces.
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
        // Types génériques chargés via l'API dynamique (DynamicObject).
        other => {
            // Résumé spécifique au type, extrait du JSON brut.
            let summary_of = |kind: &str, data: &serde_json::Value| -> String {
                match kind {
                    "endpoints" => {
                        let n = data
                            .get("subsets")
                            .and_then(|v| v.as_array())
                            .map(|ss| {
                                ss.iter()
                                    .map(|s| {
                                        s.get("addresses")
                                            .and_then(|a| a.as_array())
                                            .map(|a| a.len())
                                            .unwrap_or(0)
                                    })
                                    .sum::<usize>()
                            })
                            .unwrap_or(0);
                        format!("{n} adresse(s)")
                    }
                    "pvs" => {
                        let phase = data
                            .get("status")
                            .and_then(|s| s.get("phase"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let capacity = data
                            .get("spec")
                            .and_then(|s| s.get("capacity"))
                            .and_then(|c| c.get("storage"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let claim = data
                            .get("spec")
                            .and_then(|s| s.get("claimRef"))
                            .and_then(|c| c.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        format!("{phase} · {capacity} · {claim}")
                    }
                    "storageclasses" => {
                        let prov = data
                            .get("provisioner")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let default = data
                            .get("metadata")
                            .and_then(|m| m.get("annotations"))
                            .and_then(|a| a.get("storageclass.kubernetes.io/is-default-class"))
                            .and_then(|v| v.as_str())
                            .map(|s| s == "true")
                            .unwrap_or(false);
                        format!("{}{}", if default { "⭐ " } else { "" }, prov)
                    }
                    "serviceaccounts" => {
                        let secrets = data
                            .get("secrets")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        format!("{secrets} secret(s)")
                    }
                    "roles" | "clusterroles" => {
                        let rules = data
                            .get("rules")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        // Première règle : verbs + ressources.
                        let first = data
                            .get("rules")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.first())
                            .map(|r| {
                                let verbs = r
                                    .get("verbs")
                                    .and_then(|v| v.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|x| x.as_str())
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                    .unwrap_or_default();
                                let res = r
                                    .get("resources")
                                    .and_then(|v| v.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|x| x.as_str())
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                    .unwrap_or_default();
                                format!("{verbs} sur {res}")
                            })
                            .unwrap_or_default();
                        format!("{rules} règle(s) · {first}")
                    }
                    "rolebindings" | "clusterrolebindings" => {
                        let role = data
                            .get("roleRef")
                            .and_then(|r| r.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let subjects = data
                            .get("subjects")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        format!("{role} → {subjects}")
                    }
                    "hpas" => {
                        let min = data
                            .get("spec")
                            .and_then(|s| s.get("minReplicas"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let max = data
                            .get("spec")
                            .and_then(|s| s.get("maxReplicas"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let current = data
                            .get("status")
                            .and_then(|s| s.get("currentReplicas"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        format!("{current} réplicas ({min}–{max})")
                    }
                    "pdbs" => {
                        let disruptions = data
                            .get("status")
                            .and_then(|s| s.get("disruptionsAllowed"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        format!("{disruptions} interruption(s) autorisée(s)")
                    }
                    "networkpolicies" => {
                        let rules = data
                            .get("spec")
                            .map(|s| {
                                ["ingress", "egress"]
                                    .iter()
                                    .filter_map(|k| s.get(k))
                                    .filter_map(|v| v.as_array())
                                    .map(|a| a.len())
                                    .sum::<usize>()
                            })
                            .unwrap_or(0);
                        format!("{rules} règle(s)")
                    }
                    "resourcequotas" => {
                        let hard = data
                            .get("status")
                            .and_then(|s| s.get("hard"))
                            .and_then(|v| v.as_object())
                            .map(|m| {
                                m.iter()
                                    .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("?")))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        if hard.is_empty() {
                            "—".into()
                        } else {
                            hard
                        }
                    }
                    "namespaces" => {
                        let phase = data
                            .get("status")
                            .and_then(|s| s.get("phase"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Active");
                        phase.into()
                    }
                    _ => String::new(),
                }
            };

            let api = dynamic_api(client, other)?;
            let list: Vec<DynamicObject> = api.list(&ListParams::default()).await?.items;
            list.into_iter()
                .map(|o| {
                    let summary = summary_of(other, &o.data);
                    ResourceInfo {
                        name: o.metadata.name.clone().unwrap_or_default(),
                        namespace: o.metadata.namespace.clone().unwrap_or_default(),
                        summary: if summary.is_empty() {
                            "—".into()
                        } else {
                            summary
                        },
                        age: age_from_meta(&o, now),
                    }
                })
                .collect()
        }
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

// ---------------------------------------------------------------------------
// Helpers dynamiques (API générique via DynamicObject)
// ---------------------------------------------------------------------------

use kube::api::{ApiResource, DynamicObject};

/// Traduit un type d'URL (« statefulsets », « storageclasses »…) en
/// ApiResource (group/version/kind). Renvoie aussi l'étendue.
fn api_resource_for(kind: &str) -> Result<(ApiResource, bool)> {
    let (group, version, gkind, cluster) = match kind {
        "pods" => ("", "v1", "Pod", false),
        "services" => ("", "v1", "Service", false),
        "configmaps" => ("", "v1", "ConfigMap", false),
        "secrets" => ("", "v1", "Secret", false),
        "serviceaccounts" => ("", "v1", "ServiceAccount", false),
        "endpoints" => ("", "v1", "Endpoints", false),
        "pvs" => ("", "v1", "PersistentVolume", true),
        "pvcs" => ("", "v1", "PersistentVolumeClaim", false),
        "namespaces" => ("", "v1", "Namespace", true),
        "resourcequotas" => ("", "v1", "ResourceQuota", false),
        "deployments" => ("apps", "v1", "Deployment", false),
        "statefulsets" => ("apps", "v1", "StatefulSet", false),
        "daemonsets" => ("apps", "v1", "DaemonSet", false),
        "replicasets" => ("apps", "v1", "ReplicaSet", false),
        "jobs" => ("batch", "v1", "Job", false),
        "cronjobs" => ("batch", "v1", "CronJob", false),
        "ingresses" => ("networking.k8s.io", "v1", "Ingress", false),
        "networkpolicies" => ("networking.k8s.io", "v1", "NetworkPolicy", false),
        "storageclasses" => ("storage.k8s.io", "v1", "StorageClass", true),
        "roles" => ("rbac.authorization.k8s.io", "v1", "Role", false),
        "clusterroles" => ("rbac.authorization.k8s.io", "v1", "ClusterRole", true),
        "rolebindings" => ("rbac.authorization.k8s.io", "v1", "RoleBinding", false),
        "clusterrolebindings" => (
            "rbac.authorization.k8s.io",
            "v1",
            "ClusterRoleBinding",
            true,
        ),
        "hpas" => ("autoscaling", "v2", "HorizontalPodAutoscaler", false),
        "pdbs" => ("policy", "v1", "PodDisruptionBudget", false),
        other => anyhow::bail!("type de ressource inconnu : {other}"),
    };
    Ok((
        ApiResource::from_gvk(&kube::core::GroupVersionKind {
            group: group.into(),
            version: version.into(),
            kind: gkind.into(),
        }),
        cluster,
    ))
}

/// API dynamique pour un type, au niveau cluster (tous les namespaces
/// pour les ressources namées, direct pour les cluster-scoped).
fn dynamic_api(client: &Client, kind: &str) -> Result<Api<DynamicObject>> {
    let (ar, _) = api_resource_for(kind)?;
    Ok(Api::all_with(client.clone(), &ar))
}

/// API dynamique d'un namespace précis.
fn dynamic_api_ns(client: &Client, kind: &str, namespace: &str) -> Result<Api<DynamicObject>> {
    let (ar, _) = api_resource_for(kind)?;
    Ok(Api::namespaced_with(client.clone(), namespace, &ar))
}

/// Analyse une quantité Kubernetes en milli-CPU (« 100m », « 0.5 »).
pub(crate) fn parse_cpu_millis(s: &str) -> u64 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix('m') {
        v.trim()
            .parse::<f64>()
            .map(|f| f.max(0.0) as u64)
            .unwrap_or(0)
    } else {
        s.parse::<f64>()
            .map(|f| (f * 1000.0).max(0.0) as u64)
            .unwrap_or(0)
    }
}

/// Analyse une quantité Kubernetes en octets (« 64Mi », « 1Gi », « 512Ki »…).
pub(crate) fn parse_mem_bytes(s: &str) -> u64 {
    let s = s.trim();
    let mult = if let Some(v) = s.strip_suffix("Ki") {
        Some((v, 1024f64))
    } else if let Some(v) = s.strip_suffix("Mi") {
        Some((v, 1024.0 * 1024.0))
    } else if let Some(v) = s.strip_suffix("Gi") {
        Some((v, 1024.0 * 1024.0 * 1024.0))
    } else if let Some(v) = s.strip_suffix("Ti") {
        Some((v, 1024.0 * 1024.0 * 1024.0 * 1024.0))
    } else if let Some(v) = s.strip_suffix('K') {
        Some((v, 1e3))
    } else if let Some(v) = s.strip_suffix('M') {
        Some((v, 1e6))
    } else if let Some(v) = s.strip_suffix('G') {
        Some((v, 1e9))
    } else if let Some(v) = s.strip_suffix('T') {
        Some((v, 1e12))
    } else {
        Some((s, 1.0))
    };
    match mult {
        Some((v, m)) => v
            .trim()
            .parse::<f64>()
            .map(|f| (f * m).max(0.0) as u64)
            .unwrap_or(0),
        None => 0,
    }
}

fn age_from_meta(obj: &DynamicObject, now: chrono::DateTime<chrono::Utc>) -> String {
    obj.metadata
        .creation_timestamp
        .as_ref()
        .map(|t| human_age((now - t.0).num_seconds().max(0)))
        .unwrap_or_else(|| "—".into())
}

// ---------------------------------------------------------------------------
// Workloads : StatefulSet, DaemonSet, ReplicaSet, Job, CronJob
// ---------------------------------------------------------------------------

/// Liste un type de workload via l'API dynamique.
pub async fn list_workloads(client: &Client, kind: &str) -> Result<Vec<WorkloadInfo>> {
    use chrono::Utc;

    let api = dynamic_api(client, kind)?;
    let items: Vec<DynamicObject> = api.list(&ListParams::default()).await?.items;
    let now = Utc::now();

    Ok(items
        .into_iter()
        .map(|o| {
            let data = &o.data;
            let meta = &o.metadata;
            let spec = data.get("spec").cloned().unwrap_or_default();
            let status = data.get("status").cloned().unwrap_or_default();

            let gkind = o.types.as_ref().map(|t| t.kind.clone()).unwrap_or_default();

            // Image du premier conteneur du template.
            let image = spec
                .get("template")
                .and_then(|t| t.get("spec"))
                .and_then(|s| s.get("containers"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.get("image"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // Réplicas : selon le kind.
            let (ready, desired, summary) = match gkind.as_str() {
                "Job" => {
                    let desired = spec
                        .get("completions")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1) as i32;
                    let ready = status
                        .get("succeeded")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    let parallel = spec
                        .get("parallelism")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1);
                    (ready, desired, format!("parallélisme {parallel}"))
                }
                "CronJob" => {
                    let sched = spec
                        .get("schedule")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let suspend = spec
                        .get("suspend")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let active = status
                        .get("active")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    (
                        0,
                        0,
                        format!(
                            "{sched} · {}actif(s) : {active}",
                            if suspend { "suspendu · " } else { "" }
                        ),
                    )
                }
                _ => {
                    let desired = status
                        .get("replicas")
                        .and_then(|v| v.as_i64())
                        .or_else(|| spec.get("replicas").and_then(|v| v.as_i64()))
                        .unwrap_or(0) as i32;
                    let ready = status
                        .get("readyReplicas")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    (ready, desired, String::new())
                }
            };

            WorkloadInfo {
                kind: gkind,
                name: meta.name.clone().unwrap_or_default(),
                namespace: meta.namespace.clone().unwrap_or_default(),
                replicas_ready: ready,
                replicas_desired: desired,
                image,
                summary,
                age: age_from_meta(&o, now),
            }
        })
        .collect())
}

/// Supprime un workload.
pub async fn delete_workload(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<()> {
    dynamic_api_ns(client, kind, namespace)?
        .delete(name, &DeleteParams::default())
        .await?;
    Ok(())
}

/// Change les réplicas d'un workload (StatefulSet, ReplicaSet).
pub async fn scale_workload(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
    replicas: i32,
) -> Result<()> {
    use kube::api::{Patch, PatchParams};

    let api = dynamic_api_ns(client, kind, namespace)?;
    let patch = serde_json::json!({ "spec": { "replicas": replicas } });
    api.patch(name, &PatchParams::default(), &Patch::Strategic(&patch))
        .await?;
    Ok(())
}

/// Rollout restart d'un workload (StatefulSet, DaemonSet).
pub async fn rollout_restart_workload(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<()> {
    use kube::api::{Patch, PatchParams};

    let api = dynamic_api_ns(client, kind, namespace)?;
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
    api.patch(name, &PatchParams::default(), &Patch::Strategic(&patch))
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Fiche pod détaillée (conditions, volumes, probes, env, requests/limits)
// ---------------------------------------------------------------------------

/// Formate une probe en ligne lisible.
fn format_probe(name: &str, p: &serde_json::Value) -> Option<String> {
    use serde_json::Value;

    let detail = if let Some(h) = p.get("httpGet") {
        let path = h.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let port = h
            .get("port")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => "?".into(),
            })
            .unwrap_or_default();
        format!("httpGet {path}:{port}")
    } else if let Some(t) = p.get("tcpSocket") {
        let port = t.get("port").map(|v| v.to_string()).unwrap_or_default();
        format!("tcpSocket {port}")
    } else if let Some(e) = p.get("exec") {
        let cmd = e
            .get("command")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        format!("exec {cmd}")
    } else if let Some(g) = p.get("grpc") {
        format!(
            "grpc {}",
            g.get("port").map(|v| v.to_string()).unwrap_or_default()
        )
    } else {
        None?
    };

    let delay = p
        .get("initialDelaySeconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let period = p.get("periodSeconds").and_then(|v| v.as_i64()).unwrap_or(0);
    Some(format!(
        "{name} : {detail} (délai {delay}s, période {period}s)"
    ))
}

/// Charge la fiche complète d'un pod.
pub async fn pod_detail(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<crate::events::PodDetailInfo> {
    use crate::events::{
        ConditionInfo, PodContainerDetail, PodDetailInfo, PodVolumeInfo, TolerationInfo,
    };

    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod = pods.get(name).await?;
    let meta = &pod.metadata;
    let spec = pod.spec.clone().unwrap_or_default();
    let status = pod.status.clone().unwrap_or_default();
    let statuses = status.container_statuses.clone().unwrap_or_default();

    // Environnement et montages de chaque conteneur (spec ∩ status).
    let containers: Vec<PodContainerDetail> = spec
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
                _ => ("waiting".to_string(), String::new()),
            };

            // Variables d'env : littérales + références formatées.
            let env: Vec<(String, String)> = c
                .env
                .iter()
                .flatten()
                .map(|e| {
                    let value = if let Some(v) = &e.value {
                        v.clone()
                    } else if let Some(cf) = &e.value_from {
                        if let Some(r) = &cf.config_map_key_ref {
                            format!("← configMap/{}/{}", r.name, r.key)
                        } else if let Some(r) = &cf.secret_key_ref {
                            format!("← secret/{}/{}", r.name, r.key)
                        } else if let Some(r) = &cf.field_ref {
                            format!("← field {}", r.field_path)
                        } else {
                            "← référence".into()
                        }
                    } else {
                        String::new()
                    };
                    (e.name.clone(), value)
                })
                .collect();

            let requests: Vec<(String, String)> = c
                .resources
                .as_ref()
                .and_then(|r| r.requests.as_ref())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect())
                .unwrap_or_default();
            let limits: Vec<(String, String)> = c
                .resources
                .as_ref()
                .and_then(|r| r.limits.as_ref())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect())
                .unwrap_or_default();

            let mut probes = Vec::new();
            if let Some(p) = &c.liveness_probe {
                if let Some(line) =
                    format_probe("liveness", &serde_json::to_value(p).unwrap_or_default())
                {
                    probes.push(line);
                }
            }
            if let Some(p) = &c.readiness_probe {
                if let Some(line) =
                    format_probe("readiness", &serde_json::to_value(p).unwrap_or_default())
                {
                    probes.push(line);
                }
            }
            if let Some(p) = &c.startup_probe {
                if let Some(line) =
                    format_probe("startup", &serde_json::to_value(p).unwrap_or_default())
                {
                    probes.push(line);
                }
            }

            // Montages : nom + chemin (le type est dans pod.volumes).
            let mounts: Vec<String> = c
                .volume_mounts
                .iter()
                .flatten()
                .map(|m| format!("{} ← {}", m.mount_path, m.name))
                .collect();

            PodContainerDetail {
                name: c.name.clone(),
                image: c.image.clone().unwrap_or_default(),
                ready: s.map(|s| s.ready).unwrap_or(false),
                restarts: s.map(|s| s.restart_count).unwrap_or(0),
                state,
                reason,
                env,
                requests,
                limits,
                probes,
                mounts,
            }
        })
        .collect();

    // Volumes : type + détail (champs aplatis dans k8s-openapi 0.25).
    let volumes: Vec<PodVolumeInfo> = spec
        .volumes
        .iter()
        .flatten()
        .map(|v| {
            let (type_, detail) = if let Some(cm) = &v.config_map {
                ("configMap", cm.name.clone())
            } else if let Some(s) = &v.secret {
                ("secret", s.secret_name.clone().unwrap_or_default())
            } else if let Some(p) = &v.persistent_volume_claim {
                ("pvc", p.claim_name.clone())
            } else if let Some(h) = &v.host_path {
                ("hostPath", h.path.clone())
            } else if v.empty_dir.is_some() {
                ("emptyDir", String::new())
            } else if v.projected.is_some() {
                ("projected", String::new())
            } else if v.downward_api.is_some() {
                ("downwardAPI", String::new())
            } else if let Some(csi) = &v.csi {
                ("csi", csi.driver.clone())
            } else {
                ("autre", String::new())
            };
            PodVolumeInfo {
                name: v.name.clone(),
                type_: type_.into(),
                detail,
            }
        })
        .collect();

    let conditions: Vec<ConditionInfo> = status
        .conditions
        .iter()
        .flatten()
        .map(|c| ConditionInfo {
            type_: c.type_.clone(),
            status: c.status.clone(),
            reason: c.reason.clone().unwrap_or_default(),
            message: c.message.clone().unwrap_or_default(),
        })
        .collect();

    let tolerations: Vec<TolerationInfo> = spec
        .tolerations
        .iter()
        .flatten()
        .map(|t| TolerationInfo {
            key: t.key.clone().unwrap_or_default(),
            operator: t.operator.clone().unwrap_or_default(),
            value: t.value.clone().unwrap_or_default(),
            effect: t.effect.clone().unwrap_or_default(),
        })
        .collect();

    let node_selector: Vec<(String, String)> = spec
        .node_selector
        .iter()
        .flat_map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect();

    let owner = meta.owner_references.iter().flatten().next().cloned();

    let qos = status.qos_class.clone().unwrap_or_default();

    Ok(PodDetailInfo {
        name: meta.name.clone().unwrap_or_default(),
        namespace: meta.namespace.clone().unwrap_or_default(),
        phase: status.phase.unwrap_or_else(|| "Unknown".into()),
        qos,
        service_account: spec.service_account_name.unwrap_or_default(),
        node: spec.node_name.unwrap_or_default(),
        pod_ip: status.pod_ip.unwrap_or_default(),
        priority: spec.priority_class_name.unwrap_or_default(),
        owner_kind: owner.as_ref().map(|o| o.kind.clone()).unwrap_or_default(),
        owner_name: owner.map(|o| o.name.clone()).unwrap_or_default(),
        labels: meta
            .labels
            .iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        annotations: meta
            .annotations
            .iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        node_selector,
        conditions,
        tolerations,
        volumes,
        containers,
    })
}

/// Supprime plusieurs pods.
pub async fn delete_pods(client: &Client, pods: &[(String, String)]) -> Result<()> {
    for (ns, name) in pods {
        let api: Api<Pod> = Api::namespaced(client.clone(), ns);
        api.delete(name, &DeleteParams::default()).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// YAML : voir / éditer / appliquer (via l'API dynamique)
// ---------------------------------------------------------------------------

/// Récupère le YAML d'une ressource (managedFields retiré pour la lisibilité).
pub async fn get_resource_yaml(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<String> {
    let cluster = api_resource_for(kind)?.1;
    let obj = if cluster {
        dynamic_api(client, kind)?.get(name).await?
    } else {
        dynamic_api_ns(client, kind, namespace)?.get(name).await?
    };
    let mut obj = obj;
    obj.metadata.managed_fields = None;
    serde_yaml::to_string(&obj).map_err(|e| anyhow::anyhow!("sérialisation YAML : {e}"))
}

/// Applique un YAML : crée la ressource ou la remplace si elle existe.
pub async fn apply_yaml(client: &Client, yaml: &str) -> Result<String> {
    use kube::api::PostParams;

    let obj: DynamicObject =
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("YAML invalide : {e}"))?;
    let types = obj
        .types
        .clone()
        .ok_or_else(|| anyhow::anyhow!("apiVersion/kind manquants dans le YAML"))?;

    // Sépare group/version et déduit l'ApiResource.
    let (group, version) = match types.api_version.split_once('/') {
        Some((g, v)) => (g, v),
        None => ("", types.api_version.as_str()),
    };
    let ar = ApiResource::from_gvk(&kube::core::GroupVersionKind {
        group: group.to_string(),
        version: version.to_string(),
        kind: types.kind.clone(),
    });
    let name = obj
        .metadata
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow::anyhow!("metadata.name manquant"))?;
    let ns = obj.metadata.namespace.clone().filter(|n| !n.is_empty());

    // Étendue cluster ou namespace. La liste reprend les kinds non-namés courants.
    let cluster_scoped = matches!(
        types.kind.as_str(),
        "Node"
            | "Namespace"
            | "PersistentVolume"
            | "StorageClass"
            | "ClusterRole"
            | "ClusterRoleBinding"
            | "CustomResourceDefinition"
            | "PriorityClass"
            | "APIService"
            | "IngressClass"
            | "RuntimeClass"
            | "VolumeAttachment"
            | "CSIDriver"
            | "CSINode"
            | "ValidatingWebhookConfiguration"
            | "MutatingWebhookConfiguration"
    );

    let api: Api<DynamicObject> = if cluster_scoped {
        Api::all_with(client.clone(), &ar)
    } else {
        Api::namespaced_with(client.clone(), ns.as_deref().unwrap_or("default"), &ar)
    };

    match api.get(&name).await {
        Ok(existing) => {
            // Conserve resourceVersion pour le remplacement.
            let mut obj = obj;
            obj.metadata.resource_version = existing.metadata.resource_version.clone();
            api.replace(&name, &PostParams::default(), &obj).await?;
            Ok(format!("{}/{} mis à jour", types.kind, name))
        }
        Err(_) => {
            api.create(&PostParams::default(), &obj).await?;
            Ok(format!("{}/{} créé", types.kind, name))
        }
    }
}

// ---------------------------------------------------------------------------
// Métriques metrics-server (metrics.k8s.io)
// ---------------------------------------------------------------------------

/// Charge CPU/mémoire des pods et nœuds via metrics.k8s.io.
/// Renvoie None si le metrics-server est absent du cluster.
pub async fn load_metrics(
    client: &Client,
) -> Result<(
    Vec<crate::events::PodMetricsInfo>,
    Vec<crate::events::NodeMetricsInfo>,
)> {
    use crate::events::{NodeMetricsInfo, PodMetricsInfo};

    // PodMetrics : listé au niveau cluster (tous les namespaces).
    let pod_ar = ApiResource::from_gvk(&kube::core::GroupVersionKind {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        kind: "PodMetrics".into(),
    });
    let node_ar = ApiResource::from_gvk(&kube::core::GroupVersionKind {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        kind: "NodeMetrics".into(),
    });

    let mut pods = Vec::new();
    if let Ok(list) = Api::<DynamicObject>::all_with(client.clone(), &pod_ar)
        .list(&ListParams::default())
        .await
    {
        for o in list.items {
            let name = o.metadata.name.clone().unwrap_or_default();
            let ns = o.metadata.namespace.clone().unwrap_or_default();
            let mut cpu = 0u64;
            let mut mem = 0u64;
            if let Some(containers) = o.data.get("containers").and_then(|c| c.as_array()) {
                for c in containers {
                    if let Some(usage) = c.get("usage") {
                        cpu += usage
                            .get("cpu")
                            .and_then(|v| v.as_str())
                            .map(parse_cpu_millis)
                            .unwrap_or(0);
                        mem += usage
                            .get("memory")
                            .and_then(|v| v.as_str())
                            .map(parse_mem_bytes)
                            .unwrap_or(0);
                    }
                }
            }
            if !name.is_empty() {
                pods.push(PodMetricsInfo {
                    key: format!("{ns}/{name}"),
                    cpu_millis: cpu,
                    mem_bytes: mem,
                });
            }
        }
    }

    let mut nodes = Vec::new();
    if let Ok(list) = Api::<DynamicObject>::all_with(client.clone(), &node_ar)
        .list(&ListParams::default())
        .await
    {
        for o in list.items {
            let name = o.metadata.name.clone().unwrap_or_default();
            let usage = o.data.get("usage").cloned().unwrap_or_default();
            nodes.push(NodeMetricsInfo {
                name,
                cpu_millis: usage
                    .get("cpu")
                    .and_then(|v| v.as_str())
                    .map(parse_cpu_millis)
                    .unwrap_or(0),
                mem_bytes: usage
                    .get("memory")
                    .and_then(|v| v.as_str())
                    .map(parse_mem_bytes)
                    .unwrap_or(0),
            });
        }
    }

    Ok((pods, nodes))
}
