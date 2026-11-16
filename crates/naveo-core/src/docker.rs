//! Client Docker (via bollard) : listes, actions et logs.

use anyhow::Result;
use bollard::container::{LogOutput, LogsOptions, RemoveContainerOptions};
use bollard::image::RemoveImageOptions;
use bollard::Docker;
use futures::TryStreamExt;

use crate::events::{ContainerInfo, ImageInfo, NetworkInfo, VolumeInfo};

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
