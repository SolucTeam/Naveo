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

/// Collecte un flux d'octets bollard (export, download…) dans un Vec.
async fn body_to_vec<S, B>(mut stream: S) -> Result<Vec<u8>>
where
    S: futures::Stream<Item = std::result::Result<B, bollard::errors::Error>> + Unpin,
    B: AsRef<[u8]>,
{
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        out.extend_from_slice(chunk.as_ref());
    }
    Ok(out)
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

pub async fn container_logs(
    docker: &Docker,
    name: &str,
    timestamps: bool,
    tail: i64,
) -> Result<String> {
    let opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: if tail > 0 {
            tail.to_string()
        } else {
            "all".into()
        },
        timestamps,
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

// ---------------------------------------------------------------------------
// Pull / Docker Hub
// ---------------------------------------------------------------------------

/// Dépôt trouvé sur Docker Hub via l'API publique de recherche.
#[derive(serde::Deserialize)]
struct HubSearchResponse {
    results: Vec<HubSearchRepo>,
}

#[derive(serde::Deserialize)]
struct HubSearchRepo {
    #[serde(default)]
    repo_name: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    star_count: u64,
    #[serde(default)]
    pull_count: u64,
    #[serde(default)]
    is_official: bool,
}

/// Recherche des images sur Docker Hub (API publique v2).
pub async fn hub_search(query: &str) -> Result<Vec<crate::events::HubRepo>> {
    let url = format!("https://hub.docker.com/v2/search/repositories/?query={query}&page_size=20");
    let resp: HubSearchResponse = reqwest::get(&url).await?.json().await?;

    Ok(resp
        .results
        .into_iter()
        .map(|r| crate::events::HubRepo {
            name: r.repo_name,
            description: r.short_description,
            stars: r.star_count,
            pulls: r.pull_count,
            official: r.is_official,
        })
        .collect())
}

/// Tags d'un dépôt Docker Hub (ex. « library/nginx » ou « user/img »).
pub async fn hub_tags(repo: &str) -> Result<Vec<String>> {
    let url = format!("https://hub.docker.com/v2/repositories/{repo}/tags/?page_size=25");
    #[derive(serde::Deserialize)]
    struct Resp {
        results: Vec<Tag>,
    }
    #[derive(serde::Deserialize)]
    struct Tag {
        name: String,
    }
    let resp: Resp = reqwest::get(&url).await?.json().await?;
    Ok(resp.results.into_iter().map(|t| t.name).collect())
}

/// Tire une image (bloc jusqu'à la fin du téléchargement).
pub async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
    use futures::StreamExt as _;
    let options = bollard::image::CreateImageOptions {
        from_image: image.to_string(),
        ..Default::default()
    };
    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(ev) = stream.next().await {
        ev?; // remonte les erreurs (image introuvable…)
    }
    Ok(())
}

/// Crée et démarre un conteneur à partir d'une image.
pub async fn run_container(
    docker: &Docker,
    image: &str,
    name: &str,
    env: &[(String, String)],
    ports: &[(String, String)],
) -> Result<String> {
    use std::collections::HashMap;

    let env_vars: Vec<String> = env
        .iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    // Bindings de ports : conteneur → hôte.
    let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
    let mut bindings: HashMap<String, Vec<bollard::secret::PortBinding>> = HashMap::new();
    for (cport, hport) in ports {
        if cport.is_empty() || hport.is_empty() {
            continue;
        }
        exposed.entry(cport.clone()).or_default();
        bindings
            .entry(cport.clone())
            .or_default()
            .push(bollard::secret::PortBinding {
                host_ip: None,
                host_port: Some(hport.clone()),
            });
    }

    let host_config = bollard::models::HostConfig {
        port_bindings: Some(bindings.into_iter().map(|(k, v)| (k, Some(v))).collect()),
        ..Default::default()
    };

    let options = bollard::container::Config {
        image: Some(image.to_string()),
        env: Some(env_vars),
        exposed_ports: Some(exposed),
        host_config: Some(host_config),
        ..Default::default()
    };

    let opts = bollard::container::CreateContainerOptions {
        name: name.to_string(),
        platform: None,
    };
    let created = docker.create_container(Some(opts), options).await?;
    docker
        .start_container(
            &created.id,
            None::<bollard::container::StartContainerOptions<String>>,
        )
        .await?;
    Ok(created.id)
}

// ---------------------------------------------------------------------------
// Prune / actions en masse
// ---------------------------------------------------------------------------

/// Résultat d'un prune : nombre d'éléments et octets récupérés.
pub struct PruneResult {
    pub deleted: u64,
    pub space_reclaimed: u64,
}

/// Nettoie les ressources inutilisées : containers|images|volumes|networks.
pub async fn prune(docker: &Docker, kind: &str) -> Result<PruneResult> {
    #[allow(unused_variables)]
    let (deleted, space) = match kind {
        "containers" => {
            let r = docker
                .prune_containers(None::<bollard::container::PruneContainersOptions<String>>)
                .await?;
            (
                r.containers_deleted.map(|v| v.len()).unwrap_or(0) as u64,
                r.space_reclaimed.unwrap_or(0) as u64,
            )
        }
        "images" => {
            let r = docker
                .prune_images(None::<bollard::image::PruneImagesOptions<String>>)
                .await?;
            (
                r.images_deleted.map(|v| v.len()).unwrap_or(0) as u64,
                r.space_reclaimed.unwrap_or(0) as u64,
            )
        }
        "volumes" => {
            let r = docker
                .prune_volumes(None::<bollard::volume::PruneVolumesOptions<String>>)
                .await?;
            (
                r.volumes_deleted.map(|v| v.len()).unwrap_or(0) as u64,
                r.space_reclaimed.unwrap_or(0) as u64,
            )
        }
        "networks" => {
            let r = docker
                .prune_networks(None::<bollard::network::PruneNetworksOptions<String>>)
                .await?;
            (r.networks_deleted.map(|v| v.len()).unwrap_or(0) as u64, 0)
        }
        _ => anyhow::bail!("type de prune inconnu : {kind}"),
    };
    Ok(PruneResult {
        deleted,
        space_reclaimed: space,
    })
}

/// Applique une action à plusieurs conteneurs.
pub async fn bulk_containers(docker: &Docker, names: &[String], action: &str) -> Result<()> {
    for name in names {
        match action {
            "start" => start_container(docker, name).await?,
            "stop" => stop_container(docker, name).await?,
            "restart" => restart_container(docker, name).await?,
            "pause" => pause_container(docker, name).await?,
            "remove" => remove_container(docker, name).await?,
            _ => anyhow::bail!("action inconnue : {action}"),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Gestion d'images avancée
// ---------------------------------------------------------------------------

/// Applique un nouveau tag à une image.
pub async fn tag_image(docker: &Docker, image: &str, tag: &str) -> Result<()> {
    use bollard::image::TagImageOptions;
    let opts = TagImageOptions {
        repo: tag.split(':').next().unwrap_or(tag).to_string(),
        tag: tag
            .split_once(':')
            .map(|(_, t)| t.to_string())
            .unwrap_or_default(),
    };
    docker.tag_image(image, Some(opts)).await?;
    Ok(())
}

/// Exporte une image vers un fichier tar (docker save).
pub async fn save_image(docker: &Docker, image: &str, path: &str) -> Result<()> {
    let bytes = body_to_vec(docker.export_image(image)).await?;
    tokio::fs::write(path, &bytes).await?;
    Ok(())
}

/// Importe une image depuis un fichier tar (docker load).
pub async fn load_image(docker: &Docker, path: &str) -> Result<String> {
    use futures::StreamExt as _;
    let data = tokio::fs::read(path).await?;
    let mut stream = docker.import_image(
        bollard::image::ImportImageOptions::default(),
        data.into(),
        None,
    );
    let mut summary = String::new();
    while let Some(chunk) = stream.next().await {
        let info = chunk?;
        if let Some(err) = info.error {
            anyhow::bail!("import : {err}");
        }
        if let Some(status) = info.status {
            summary.push_str(&status);
            summary.push('\n');
        }
    }
    Ok(summary)
}

/// Historique des couches d'une image.
pub async fn image_history(
    docker: &Docker,
    image: &str,
) -> Result<Vec<crate::events::ImageLayerInfo>> {
    let history = docker.image_history(image).await?;
    Ok(history
        .into_iter()
        .map(|h| crate::events::ImageLayerInfo {
            created_by: h.created_by,
            size_bytes: h.size.max(0) as u64,
            created: h.created,
        })
        .collect())
}

/// Connecte un registre : écrit ~/.docker/config.json comme la CLI.
pub async fn registry_login(server: &str, username: &str, password: &str) -> Result<()> {
    use base64::Engine as _;
    let path = docker_config_path()?;
    let mut cfg: serde_json::Value = match tokio::fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str(&s)?,
        Err(_) => serde_json::json!({}),
    };
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    let auths = cfg
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("config.json invalide"))?
        .entry("auths")
        .or_insert_with(|| serde_json::json!({}));
    auths
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("auths invalide"))?
        .insert(
            server.to_string(),
            serde_json::json!({ "auth": auth, "username": username }),
        );
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_string_pretty(&cfg)?).await?;
    Ok(())
}

/// Déconnecte un registre.
pub async fn registry_logout(server: &str) -> Result<()> {
    let path = docker_config_path()?;
    let Ok(s) = tokio::fs::read_to_string(&path).await else {
        return Ok(());
    };
    let mut cfg: serde_json::Value = serde_json::from_str(&s)?;
    if let Some(auths) = cfg.get_mut("auths").and_then(|a| a.as_object_mut()) {
        auths.remove(server);
    }
    tokio::fs::write(&path, serde_json::to_string_pretty(&cfg)?).await?;
    Ok(())
}

fn docker_config_path() -> Result<std::path::PathBuf> {
    Ok(home::home_dir()
        .ok_or_else(|| anyhow::anyhow!("home introuvable"))?
        .join(".docker")
        .join("config.json"))
}

// ---------------------------------------------------------------------------
// Fichiers du conteneur (docker cp)
// ---------------------------------------------------------------------------

/// Liste un répertoire du conteneur en lisant l'archive tar retournée
/// par l'API « download from container » (docker cp).
pub async fn list_files(
    docker: &Docker,
    name: &str,
    path: &str,
) -> Result<Vec<crate::events::ContainerFile>> {
    use tar::Archive;

    let stream = docker.download_from_container(
        name,
        Some(bollard::container::DownloadFromContainerOptions {
            path: path.to_string(),
        }),
    );
    let bytes = body_to_vec(stream).await?;

    let mut entries = Vec::new();
    let mut archive = Archive::new(&bytes[..]);
    for entry in archive.entries()? {
        let entry = entry?;
        let name = entry
            .path()?
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.header().size()?;
        let mode = format!("{:o}", entry.header().mode().unwrap_or(0));
        entries.push(crate::events::ContainerFile {
            name,
            size,
            is_dir,
            mode,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// Extrait un fichier du conteneur (petit : ≤ 256 Ko) pour l'aperçu.
pub async fn preview_file(docker: &Docker, name: &str, path: &str) -> Result<String> {
    use tar::Archive;

    let stream = docker.download_from_container(
        name,
        Some(bollard::container::DownloadFromContainerOptions {
            path: path.to_string(),
        }),
    );
    let bytes = body_to_vec(stream).await?;

    let mut archive = Archive::new(&bytes[..]);
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let mut content = Vec::new();
        use std::io::Read as _;
        entry.take(256 * 1024).read_to_end(&mut content)?;
        return Ok(String::from_utf8_lossy(&content).to_string());
    }
    anyhow::bail!("fichier introuvable dans l'archive")
}

/// Sauvegarde un fichier/répertoire du conteneur vers le disque (tar).
pub async fn save_file(docker: &Docker, name: &str, path: &str, dest: &str) -> Result<()> {
    let stream = docker.download_from_container(
        name,
        Some(bollard::container::DownloadFromContainerOptions {
            path: path.to_string(),
        }),
    );
    let bytes = body_to_vec(stream).await?;
    tokio::fs::write(dest, &bytes).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Édition à chaud : rename, commit, export, recréation
// ---------------------------------------------------------------------------

/// Renomme un conteneur.
pub async fn rename_container(docker: &Docker, name: &str, new_name: &str) -> Result<()> {
    use bollard::container::RenameContainerOptions;
    docker
        .rename_container(
            name,
            RenameContainerOptions {
                name: new_name.to_string(),
            },
        )
        .await?;
    Ok(())
}

/// Commit : crée une image à partir de l'état actuel du conteneur.
pub async fn commit_container(
    docker: &Docker,
    name: &str,
    repo: &str,
    tag: &str,
) -> Result<String> {
    let opts = bollard::image::CommitContainerOptions {
        container: name.to_string(),
        repo: repo.to_string(),
        tag: tag.to_string(),
        ..Default::default()
    };
    let commit = docker
        .commit_container(opts, bollard::container::Config::<String>::default())
        .await?;
    Ok(commit.id.unwrap_or_default())
}

/// Exporte le système de fichiers du conteneur en tar.
pub async fn export_container(docker: &Docker, name: &str, path: &str) -> Result<()> {
    let bytes = body_to_vec(docker.export_container(name)).await?;
    tokio::fs::write(path, &bytes).await?;
    Ok(())
}

/// Recrée un conteneur : inspecte l'ancien, crée un nouveau avec les
/// nouvelles env/ports, le démarre, supprime l'ancien.
pub async fn recreate_container(
    docker: &Docker,
    name: &str,
    env: &[(String, String)],
    ports: &[(String, String)],
) -> Result<()> {
    use std::collections::HashMap;

    let old = docker.inspect_container(name, None).await?;
    let old_config = old
        .config
        .clone()
        .ok_or_else(|| anyhow::anyhow!("conteneur sans configuration"))?;
    let image = old_config
        .image
        .clone()
        .ok_or_else(|| anyhow::anyhow!("conteneur sans image"))?;

    let env_vars: Vec<String> = env
        .iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
    let mut bindings: HashMap<String, Vec<bollard::secret::PortBinding>> = HashMap::new();
    for (cport, hport) in ports {
        if cport.is_empty() || hport.is_empty() {
            continue;
        }
        exposed.entry(cport.clone()).or_default();
        bindings
            .entry(cport.clone())
            .or_default()
            .push(bollard::secret::PortBinding {
                host_ip: None,
                host_port: Some(hport.clone()),
            });
    }

    // Conserve la commande, le point d'entrée, les volumes et le réseau.
    let mut new_config: bollard::container::Config<String> = bollard::container::Config {
        image: Some(image.clone()),
        cmd: old_config.cmd.clone(),
        entrypoint: old_config.entrypoint.clone(),
        working_dir: old_config.working_dir.clone(),
        user: old_config.user.clone(),
        tty: old_config.tty,
        open_stdin: old_config.open_stdin,
        env: Some(env_vars),
        exposed_ports: Some(exposed),
        ..Default::default()
    };

    let mut host_config: bollard::models::HostConfig = old.host_config.clone().unwrap_or_default();
    host_config.port_bindings = Some(bindings.into_iter().map(|(k, v)| (k, Some(v))).collect());
    new_config.host_config = Some(host_config);

    // Ancien nom sans le '/' initial.
    let old_name = name.trim_start_matches('/').to_string();
    let tmp_name = format!("{}-naveo-recreate", old_name);

    let created = docker
        .create_container(
            Some(bollard::container::CreateContainerOptions {
                name: tmp_name.clone(),
                platform: None,
            }),
            new_config,
        )
        .await?;

    // Arrête et supprime l'ancien (le nom est libéré), puis renomme.
    let _ = stop_container(docker, name).await;
    remove_container(docker, name).await?;
    docker
        .rename_container(
            &created.id,
            bollard::container::RenameContainerOptions { name: old_name },
        )
        .await?;
    docker
        .start_container(
            &created.id,
            None::<bollard::container::StartContainerOptions<String>>,
        )
        .await?;
    Ok(())
}
