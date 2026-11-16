//! Client Kubernetes (via kube-rs) : nœuds, pods, déploiements, logs.

use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::{DeleteParams, ListParams, LogParams};
use kube::{Api, Client};

use crate::events::{DeploymentInfo, NodeInfo, PodInfo};

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
            let status = p.status.unwrap_or_default();
            let containers = p
                .spec
                .as_ref()
                .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
                .unwrap_or_default();
            let restarts = status
                .container_statuses
                .unwrap_or_default()
                .iter()
                .map(|cs| cs.restart_count)
                .sum();

            PodInfo {
                name: p.metadata.name.unwrap_or_default(),
                namespace: p.metadata.namespace.unwrap_or_default(),
                node: status.host_ip.unwrap_or_default(),
                phase: status.phase.unwrap_or_else(|| "Unknown".into()),
                restarts,
                containers,
            }
        })
        .collect())
}

pub async fn delete_pod(client: &Client, namespace: &str, name: &str) -> Result<()> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    pods.delete(name, &DeleteParams::default()).await?;
    Ok(())
}

pub async fn pod_logs(client: &Client, namespace: &str, name: &str) -> Result<String> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let params = LogParams {
        tail_lines: Some(500),
        ..Default::default()
    };
    Ok(pods.logs(name, &params).await?)
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
