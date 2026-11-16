//! Naveo — moteur partagé : clients Docker (bollard) et Kubernetes (kube).
//!
//! `naveo-core` est indépendant de l'UI : il expose des commandes
//! ([`Command`]) exécutables en arrière-plan qui renvoient des
//! [`Event`] consommés par le thread graphique.

pub mod docker;
pub mod events;
pub mod k8s;

use anyhow::Result;
use bollard::Docker;
use crossbeam_channel::Sender;
use kube::Client;

pub use events::{
    Command, ContainerInfo, DeploymentInfo, Event, ImageInfo, NetworkInfo, NodeInfo, PodInfo,
    VolumeInfo,
};

/// Exécute une commande dans le runtime asynchrone et pousse
/// le résultat (ou une erreur) sur le canal d'événements.
pub async fn dispatch(
    cmd: Command,
    docker: Option<&Docker>,
    kube: Option<&Client>,
    tx: Sender<Event>,
) -> Result<()> {
    match cmd {
        Command::RefreshContainers => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::RefreshImages => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
        }
        Command::RefreshVolumes => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            tx.send(Event::Volumes(docker::list_volumes(docker).await?))?;
        }
        Command::RefreshNetworks => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            tx.send(Event::Networks(docker::list_networks(docker).await?))?;
        }
        Command::StartContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::start_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::StopContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::stop_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::RemoveContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::remove_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::RemoveImage(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::remove_image(docker, &name).await?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
        }
        Command::ContainerLogs(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let text = docker::container_logs(docker, &name).await?;
            tx.send(Event::Logs { target: name, text })?;
        }
        Command::RefreshK8s => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            tx.send(Event::Nodes(k8s::list_nodes(client).await?))?;
            tx.send(Event::Pods(k8s::list_pods(client).await?))?;
            tx.send(Event::Deployments(k8s::list_deployments(client).await?))?;
        }
        Command::DeletePod { namespace, name } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            k8s::delete_pod(client, &namespace, &name).await?;
            tx.send(Event::Pods(k8s::list_pods(client).await?))?;
        }
        Command::PodLogs { namespace, name, .. } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            let text = k8s::pod_logs(client, &namespace, &name).await?;
            tx.send(Event::Logs {
                target: format!("{namespace}/{name}"),
                text,
            })?;
        }
    }
    Ok(())
}
