//! Client Docker (via bollard) : listes, actions et logs.

use anyhow::Result;
use bollard::container::{LogOutput, LogsOptions, RemoveContainerOptions, StatsOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::RemoveImageOptions;
use bollard::Docker;
use crossbeam_channel::Sender;
use futures::{StreamExt, TryStreamExt};

use crate::events::{ContainerInfo, Event, ImageInfo, NetworkInfo, StatsInfo, VolumeInfo};

fn port_type(t: Option<bollard::models::PortTypeEnum>) -> &'static str {
    match t {
        Some(bollard::models::PortTypeEnum::UDP) => "udp",
        Some(bollard::models::PortTypeEnum::SCTP) => "sctp",
        _ => "tcp",
    }
}

pub async fn connect() -> Result<Docker> {
    let docker = Docker::connect_with_local_defaults()?;
    docker.ping().await?; // vérifie que le daemon répond
    Ok(docker)
}

// ---------------------------------------------------------------------------
// Listes
// ---------------------------------------------------------------------------

pub async fn list_containers(docker: &Docker) -> Result<Vec<ContainerInfo>> {
    let opts = bollard::container::ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let list = docker.list_containers(Some(opts)).await?;

    Ok(list
        .into_iter()
        .map(|c| ContainerInfo {
            id: c.id.unwrap_or_default().chars().take(12).collect(),
            name: c
                .names
                .unwrap_or_default()
                .first()
                .cloned()
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default(),
            image: c.image.unwrap_or_default(),
            state: c.state.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
            ports: c
                .ports
                .unwrap_or_default()
                .iter()
                .map(|p| match p.public_port {
                    Some(public) => format!(
                        "{}:{}→{}/{}",
                        p.ip.clone().unwrap_or_else(|| "0.0.0.0".into()),
                        public,
                        p.private_port,
                        port_type(p.typ)
                    ),
                    None => format!("{}/{}", p.private_port, port_type(p.typ)),
                })
                .collect(),
            project: c
                .labels
                .as_ref()
                .and_then(|l| l.get("com.docker.compose.project").cloned()),
        })
        .collect())
}

pub async fn list_images(docker: &Docker) -> Result<Vec<ImageInfo>> {
    let opts = bollard::image::ListImagesOptions::<String> {
        all: true,
        ..Default::default()
    };
    let list = docker.list_images(Some(opts)).await?;

    Ok(list
        .into_iter()
        .map(|i| ImageInfo {
            tags: i.repo_tags,
            size_bytes: i.size.max(0) as u64,
            created: i.created,
            id: i.id.chars().take(19).collect(),
        })
        .collect())
}

pub async fn list_volumes(docker: &Docker) -> Result<Vec<VolumeInfo>> {
    let vols = docker
        .list_volumes(None::<bollard::volume::ListVolumesOptions<String>>)
        .await?;

    Ok(vols
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| VolumeInfo {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
        })
        .collect())
}

pub async fn list_networks(docker: &Docker) -> Result<Vec<NetworkInfo>> {
    let opts = bollard::network::ListNetworksOptions::<String>::default();
    let nets = docker.list_networks(Some(opts)).await?;

    Ok(nets
        .into_iter()
        .map(|n| NetworkInfo {
            name: n.name.unwrap_or_default(),
            driver: n.driver.unwrap_or_default(),
            scope: n.scope.unwrap_or_default(),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

pub async fn start_container(docker: &Docker, name: &str) -> Result<()> {
    docker
        .start_container(
            name,
            None::<bollard::container::StartContainerOptions<String>>,
        )
        .await?;
    Ok(())
}

pub async fn stop_container(docker: &Docker, name: &str) -> Result<()> {
    docker
        .stop_container(name, None::<bollard::container::StopContainerOptions>)
        .await?;
    Ok(())
}

pub async fn restart_container(docker: &Docker, name: &str) -> Result<()> {
    docker
        .restart_container(name, None::<bollard::container::RestartContainerOptions>)
        .await?;
    Ok(())
}

pub async fn pause_container(docker: &Docker, name: &str) -> Result<()> {
    docker.pause_container(name).await?;
    Ok(())
}

pub async fn unpause_container(docker: &Docker, name: &str) -> Result<()> {
    docker.unpause_container(name).await?;
    Ok(())
}

pub async fn kill_container(docker: &Docker, name: &str) -> Result<()> {
    docker
        .kill_container(
            name,
            None::<bollard::container::KillContainerOptions<String>>,
        )
        .await?;
    Ok(())
}

/// Applique une action à tous les conteneurs d'un projet Compose.
pub async fn project_action(docker: &Docker, project: &str, action: &str) -> Result<()> {
    let opts = bollard::container::ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let list = docker.list_containers(Some(opts)).await?;

    for c in list {
        let in_project = c
            .labels
            .as_ref()
            .and_then(|l| l.get("com.docker.compose.project"))
            .is_some_and(|p| p == project);
        if !in_project {
            continue;
        }
        let Some(name) = c.names.unwrap_or_default().first().cloned() else {
            continue;
        };
        let name = name.trim_start_matches('/').to_string();
        match action {
            "start" => start_container(docker, &name).await?,
            "stop" => stop_container(docker, &name).await?,
            "restart" => restart_container(docker, &name).await?,
            "remove" => remove_container(docker, &name).await?,
            _ => anyhow::bail!("action inconnue : {action}"),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Détail : inspect, stats, logs en direct
// ---------------------------------------------------------------------------

/// Renvoie le JSON d'inspection du conteneur, indenté pour l'affichage.
pub async fn inspect_container(docker: &Docker, name: &str) -> Result<String> {
    let inspect = docker.inspect_container(name, None).await?;
    Ok(serde_json::to_string_pretty(&inspect)?)
}

/// Suit les logs d'un conteneur en direct et pousse chaque morceau
/// sur le canal d'événements. Termine quand le flux s'arrête
/// (conteneur supprimé, StopFollow…).
pub async fn stream_logs(docker: &Docker, name: &str, key: &str, tx: Sender<Event>) -> Result<()> {
    let opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: "200".into(),
        timestamps: false,
        follow: true,
        since: 0,
        until: 0,
    };

    let mut stream = docker.logs(name, Some(opts));
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        let bytes: &[u8] = match &chunk {
            LogOutput::StdOut { message } => message,
            LogOutput::StdErr { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        };
        let text = String::from_utf8_lossy(bytes).to_string();
        if tx
            .send(Event::LogChunk {
                target: key.into(),
                text,
            })
            .is_err()
        {
            break; // l'UI est fermée
        }
    }
    Ok(())
}

/// Suit les stats d'un conteneur (CPU, mémoire, réseau) en direct.
pub async fn stream_stats(docker: &Docker, name: &str, tx: Sender<Event>) -> Result<()> {
    let opts = StatsOptions {
        stream: true,
        one_shot: false,
    };

    let mut stream = docker.stats(name, Some(opts));
    while let Some(stats) = stream.next().await {
        let Ok(stats) = stats else { break };

        // Formule du CPU telle que la CLI Docker la calcule.
        let cpu_delta = stats
            .cpu_stats
            .cpu_usage
            .total_usage
            .saturating_sub(stats.precpu_stats.cpu_usage.total_usage);
        let system_delta = stats
            .cpu_stats
            .system_cpu_usage
            .unwrap_or(0)
            .saturating_sub(stats.precpu_stats.system_cpu_usage.unwrap_or(0));
        let cpus = stats.cpu_stats.online_cpus.unwrap_or(1).max(1) as f64;
        let cpu_percent = if system_delta > 0 {
            (cpu_delta as f64 / system_delta as f64) * cpus * 100.0
        } else {
            0.0
        };

        let mem_used = stats.memory_stats.usage.unwrap_or(0);
        let mem_limit = stats.memory_stats.limit.unwrap_or(0);
        let mem_percent = if mem_limit > 0 {
            mem_used as f64 / mem_limit as f64 * 100.0
        } else {
            0.0
        };

        let (mut rx, mut tx_bytes) = (0u64, 0u64);
        for net in stats.networks.unwrap_or_default().values() {
            rx += net.rx_bytes;
            tx_bytes += net.tx_bytes;
        }

        let info = StatsInfo {
            cpu_percent,
            mem_used_bytes: mem_used,
            mem_limit_bytes: mem_limit,
            mem_percent,
            net_rx_bytes: rx,
            net_tx_bytes: tx_bytes,
        };
        if tx
            .send(Event::Stats {
                name: name.into(),
                info,
            })
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

pub async fn remove_container(docker: &Docker, name: &str) -> Result<()> {
    let opts = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    docker.remove_container(name, Some(opts)).await?;
    Ok(())
}

pub async fn remove_image(docker: &Docker, name: &str) -> Result<()> {
    let opts = RemoveImageOptions {
        force: true,
        ..Default::default()
    };
    docker.remove_image(name, Some(opts), None).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Exec (terminal)
// ---------------------------------------------------------------------------

/// Exécute `sh -c <cmd>` dans le conteneur et renvoie la sortie combinée.
/// Chaque commande crée un nouvel exec : pas de session PTY persistante,
/// mais fiable et suffisant pour un terminal « par commande ».
pub async fn exec_container(docker: &Docker, name: &str, cmd: &str) -> Result<String> {
    let exec = docker
        .create_exec(
            name,
            CreateExecOptions::<String> {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(vec!["sh".into(), "-c".into(), cmd.into()]),
                ..Default::default()
            },
        )
        .await?;

    let mut out = String::new();
    match docker.start_exec(&exec.id, None).await? {
        StartExecResults::Attached { mut output, .. } => {
            while let Some(msg) = output.next().await {
                match msg? {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                        out.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }
        StartExecResults::Detached => {}
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

pub async fn container_logs(docker: &Docker, name: &str) -> Result<String> {
    let opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: "500".into(),
        timestamps: false,
        follow: false,
        since: 0,
        until: 0,
    };

    let mut out = String::new();
    let mut stream = docker.logs(name, Some(opts));
    while let Some(chunk) = stream.try_next().await? {
        let bytes: &[u8] = match &chunk {
            LogOutput::StdOut { message } => message,
            LogOutput::StdErr { message } => message,
            LogOutput::StdIn { message } => message,
            LogOutput::Console { message } => message,
        };
        out.push_str(&String::from_utf8_lossy(bytes));
    }
    Ok(out)
}
