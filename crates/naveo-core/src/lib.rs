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
    Command, ContainerFile, ContainerInfo, ContextInfo, DeploymentInfo, Event, HubRepo, ImageInfo,
    ImageLayerInfo, K8sEventInfo, NetworkInfo, NodeInfo, PodInfo, ResourceInfo, ServiceInfo,
    StatsInfo, VolumeInfo,
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
        Command::RestartContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::restart_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::PauseContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::pause_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::UnpauseContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::unpause_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::KillContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::kill_container(docker, &name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::ProjectAction { project, action } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::project_action(docker, &project, &action).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::InspectContainer(name) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let json = docker::inspect_container(docker, &name).await?;
            tx.send(Event::Inspect {
                target: format!("docker:{name}"),
                json,
            })?;
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
        Command::ContainerLogs {
            name,
            timestamps,
            tail,
        } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let text = docker::container_logs(docker, &name, timestamps, tail).await?;
            tx.send(Event::Logs { target: name, text })?;
        }
        Command::BulkContainers { names, action } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::bulk_containers(docker, &names, &action).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
        }
        Command::Prune { kind } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let r = docker::prune(docker, &kind).await?;
            let mb = r.space_reclaimed as f64 / 1_048_576.0;
            tx.send(Event::Notice(format!(
                "Nettoyage {kind} : {} élément(s) supprimé(s), {mb:.1} Mo libérés",
                r.deleted
            )))?;
            // Rafraîchit la vue concernée.
            match kind.as_str() {
                "containers" => {
                    tx.send(Event::Containers(docker::list_containers(docker).await?))?
                }
                "images" => tx.send(Event::Images(docker::list_images(docker).await?))?,
                "volumes" => tx.send(Event::Volumes(docker::list_volumes(docker).await?))?,
                _ => tx.send(Event::Networks(docker::list_networks(docker).await?))?,
            };
        }
        Command::HubSearch { query } => {
            tx.send(Event::HubResults(docker::hub_search(&query).await?))?;
        }
        Command::HubTags { repo } => {
            let tags = docker::hub_tags(&repo).await?;
            tx.send(Event::HubTags { repo, tags })?;
        }
        Command::PullImage { image } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            tx.send(Event::Notice(format!("Téléchargement de {image}…")))?;
            docker::pull_image(docker, &image).await?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
            tx.send(Event::Notice(format!("Image {image} récupérée")))?;
        }
        Command::RunContainer {
            image,
            name,
            env,
            ports,
        } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let id = docker::run_container(docker, &image, &name, &env, &ports).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
            tx.send(Event::Notice(format!(
                "Conteneur démarré ({})…",
                &id[..12.min(id.len())]
            )))?;
        }
        Command::TagImage { image, tag } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::tag_image(docker, &image, &tag).await?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
            tx.send(Event::Notice(format!("Nouveau tag : {tag}")))?;
        }
        Command::SaveImage { image, path } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::save_image(docker, &image, &path).await?;
            tx.send(Event::Notice(format!("Image exportée vers {path}")))?;
        }
        Command::LoadImage { path } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let _ = docker::load_image(docker, &path).await?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
            tx.send(Event::Notice(format!("Image importée depuis {path}")))?;
        }
        Command::ImageHistory(image) => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let layers = docker::image_history(docker, &image).await?;
            tx.send(Event::ImageHistory { image, layers })?;
        }
        Command::RegistryLogin {
            server,
            username,
            password,
        } => {
            docker::registry_login(&server, &username, &password).await?;
            tx.send(Event::Notice(format!("Registre {server} connecté")))?;
        }
        Command::RegistryLogout(server) => {
            docker::registry_logout(&server).await?;
            tx.send(Event::Notice(format!("Registre {server} déconnecté")))?;
        }
        Command::ListFiles { name, path } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let entries = docker::list_files(docker, &name, &path).await?;
            tx.send(Event::FileList {
                name,
                path,
                entries,
            })?;
        }
        Command::PreviewFile { name, path } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let content = docker::preview_file(docker, &name, &path).await?;
            tx.send(Event::FilePreview {
                name,
                path,
                content,
            })?;
        }
        Command::SaveFile { name, path, dest } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::save_file(docker, &name, &path, &dest).await?;
            tx.send(Event::Notice(format!("Fichier sauvegardé : {dest}")))?;
        }
        Command::RenameContainer { name, new_name } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::rename_container(docker, &name, &new_name).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
            tx.send(Event::Notice(format!("Conteneur renommé : {new_name}")))?;
        }
        Command::CommitContainer { name, repo, tag } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::commit_container(docker, &name, &repo, &tag).await?;
            tx.send(Event::Images(docker::list_images(docker).await?))?;
            tx.send(Event::Notice(format!("Commit créé : {repo}:{tag}")))?;
        }
        Command::ExportContainer { name, path } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::export_container(docker, &name, &path).await?;
            tx.send(Event::Notice(format!("Conteneur exporté : {path}")))?;
        }
        Command::RecreateContainer { name, env, ports } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            docker::recreate_container(docker, &name, &env, &ports).await?;
            tx.send(Event::Containers(docker::list_containers(docker).await?))?;
            tx.send(Event::Notice(format!(
                "Conteneur {name} recréé avec la nouvelle configuration"
            )))?;
        }
        Command::ScaleDeployment {
            namespace,
            name,
            replicas,
        } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            k8s::scale_deployment(client, &namespace, &name, replicas).await?;
            tx.send(Event::Deployments(k8s::list_deployments(client).await?))?;
            tx.send(Event::Notice(format!("{name} : {replicas} réplica(s)")))?;
        }
        Command::RolloutRestart { namespace, name } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            k8s::rollout_restart(client, &namespace, &name).await?;
            tx.send(Event::Notice(format!("Rollout restart lancé sur {name}")))?;
        }
        Command::LoadResources { kind } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            let items = k8s::load_resources(client, &kind).await?;
            tx.send(Event::Resources { kind, items })?;
        }
        Command::DeletePod { namespace, name } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            k8s::delete_pod(client, &namespace, &name).await?;
            tx.send(Event::Pods(k8s::list_pods(client).await?))?;
        }
        Command::RefreshContexts => {
            tx.send(Event::ContextsAvailable(k8s::list_contexts()))?;
        }
        Command::PodLogs {
            namespace,
            name,
            container,
            previous,
        } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            let text = k8s::pod_logs(client, &namespace, &name, Some(&container), previous).await?;
            tx.send(Event::Logs {
                target: format!("{namespace}/{name}"),
                text,
            })?;
        }
        Command::ExecContainer { name, cmd } => {
            let docker = docker.ok_or_else(|| anyhow::anyhow!("Docker non connecté"))?;
            let text = docker::exec_container(docker, &name, &cmd).await?;
            tx.send(Event::ExecOutput {
                target: format!("docker:{name}"),
                text,
            })?;
        }
        Command::PodExec {
            namespace,
            name,
            container,
            cmd,
        } => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            let text = k8s::pod_exec(client, &namespace, &name, Some(&container), &cmd).await?;
            tx.send(Event::ExecOutput {
                target: format!("k8s:{namespace}/{name}"),
                text,
            })?;
        }
        Command::RefreshK8s => {
            let client = kube.ok_or_else(|| anyhow::anyhow!("Kubernetes non connecté"))?;
            tx.send(Event::Nodes(k8s::list_nodes(client).await?))?;
            tx.send(Event::Pods(k8s::list_pods(client).await?))?;
            tx.send(Event::Deployments(k8s::list_deployments(client).await?))?;
            tx.send(Event::Services(k8s::list_services(client).await?))?;
            tx.send(Event::Events(k8s::list_events(client).await?))?;
        }
        // Géré par la tâche de fond de l'app (mutabilité du client / flux),
        // jamais atteint ici.
        Command::ConnectContext { .. } => {}
        Command::FollowLogs { .. } => {}
        Command::FollowStats(_) => {}
        Command::StopFollow { .. } => {}
        Command::PortForward { .. } => {}
        Command::StopPortForward { .. } => {}
    }
    Ok(())
}
