use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{bail, Context, Result};
use clap::Parser;
use fabdev_core::{
  create_site, default_site_domain, default_site_home, edit_site, AgentEndpoint, AgentRequest,
  AgentResponse, AgentStatus, AppPaths, LanShareInfo, LanShareSiteInfo, NodeRuntimeState,
  PhpRuntimeInfo, PhpRuntimeState, PhpVersion, RuntimeUpdateArtifact, RuntimeUpdateCheck,
  RuntimeUpdateOperation, RuntimeUpdateOperationStatus, ServiceState, Site, SiteHomeSettings,
  SiteInput, SiteRepository, PROTOCOL_VERSION, STABLE_NODE_VERSION,
};
use fabdev_proxy::ProxyManager;
use fabdev_runtime::{
  active_version, deactivate_runtime, install_tar_gz_with_activation, list_installed_versions,
  mark_runtime_removed, remove_installed_version, set_active_version, RuntimeRelease,
  ValidatedRuntimeCatalog,
};
use fabdev_services::{ensure_local_ca, RuntimePaths, ServicePorts, ServiceSupervisor};
use fabdev_share::ShareServer;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
#[cfg(windows)]
use tokio::net::windows::named_pipe::ServerOptions;
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Parser)]
#[command(
  name = "fabdev-agent",
  version,
  about = "fabDev background service agent"
)]
struct Arguments {
  #[cfg(unix)]
  #[arg(long)]
  socket: Option<PathBuf>,
  #[cfg(windows)]
  #[arg(long)]
  pipe: Option<String>,
  #[arg(long)]
  data_dir: Option<PathBuf>,
  #[arg(long)]
  dnsmasq_runtime: Option<PathBuf>,
  #[arg(long)]
  nginx_runtime: Option<PathBuf>,
  #[arg(long)]
  php_runtime: Option<PathBuf>,
  #[arg(long)]
  mariadb_runtime: Option<PathBuf>,
  #[arg(long, default_value_t = 53_535)]
  dns_port: u16,
  #[arg(long, default_value_t = default_http_port())]
  http_port: u16,
  #[arg(long, default_value_t = default_https_port())]
  https_port: u16,
  #[arg(long, default_value_t = 3_306)]
  mariadb_port: u16,
}

#[cfg(windows)]
const fn default_http_port() -> u16 {
  80
}

#[cfg(not(windows))]
const fn default_http_port() -> u16 {
  8_080
}

#[cfg(windows)]
const fn default_https_port() -> u16 {
  443
}

#[cfg(not(windows))]
const fn default_https_port() -> u16 {
  8_443
}

struct AgentState {
  paths: AppPaths,
  sites: Mutex<SiteRepository>,
  services: Mutex<ServiceSupervisor>,
  lan_share: Mutex<LanShareState>,
  proxy_manager: Mutex<ProxyManager>,
  runtime_updates: RuntimeUpdateManager,
  shutdown: Arc<Notify>,
}

#[derive(Clone, Default)]
struct RuntimeUpdateManager {
  operations: Arc<Mutex<HashMap<uuid::Uuid, Arc<RuntimeDownloadTask>>>>,
}

struct RuntimeDownloadTask {
  snapshot: StdMutex<RuntimeUpdateOperation>,
  cancelled: AtomicBool,
  bytes_downloaded: AtomicU64,
}

impl RuntimeDownloadTask {
  fn new(operation: RuntimeUpdateOperation) -> Self {
    Self {
      bytes_downloaded: AtomicU64::new(operation.bytes_downloaded),
      snapshot: StdMutex::new(operation),
      cancelled: AtomicBool::new(false),
    }
  }

  fn snapshot(&self) -> RuntimeUpdateOperation {
    let mut snapshot = self
      .snapshot
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone();
    snapshot.bytes_downloaded = self.bytes_downloaded.load(Ordering::Relaxed);
    snapshot
  }

  fn begin_download(&self) -> bool {
    let mut snapshot = self
      .snapshot
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if self.cancelled.load(Ordering::Acquire) {
      return false;
    }
    snapshot.status = RuntimeUpdateOperationStatus::Downloading;
    snapshot.error = None;
    true
  }

  fn set_progress(&self, downloaded: u64) {
    self.bytes_downloaded.store(downloaded, Ordering::Relaxed);
  }

  fn set_status(&self, status: RuntimeUpdateOperationStatus, error: Option<String>) {
    let mut snapshot = self
      .snapshot
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    snapshot.status = status;
    snapshot.error = error;
  }

  fn cancel(&self) -> Result<RuntimeUpdateOperation> {
    let mut snapshot = self
      .snapshot
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !matches!(
      snapshot.status,
      RuntimeUpdateOperationStatus::Queued | RuntimeUpdateOperationStatus::Downloading
    ) {
      bail!("Runtime download can no longer be cancelled");
    }
    self.cancelled.store(true, Ordering::Release);
    snapshot.status = RuntimeUpdateOperationStatus::Cancelled;
    snapshot.error = None;
    drop(snapshot);
    Ok(self.snapshot())
  }

  fn is_cancelled(&self) -> bool {
    self.cancelled.load(Ordering::Acquire)
  }
}

impl RuntimeUpdateManager {
  async fn start(
    &self,
    cache_directory: PathBuf,
    artifact: RuntimeUpdateArtifact,
  ) -> Result<RuntimeUpdateOperation> {
    const MAX_RUNTIME_UPDATE_OPERATIONS: usize = 64;

    let operation_id = uuid::Uuid::new_v4();
    let operation = RuntimeUpdateOperation {
      operation_id,
      status: RuntimeUpdateOperationStatus::Queued,
      name: artifact.name,
      version: artifact.version,
      platform: artifact.platform,
      architecture: artifact.architecture,
      file_name: artifact.file_name,
      bytes_downloaded: 0,
      total_bytes: artifact.size,
      sha256: artifact.sha256,
      error: None,
    };
    let task = Arc::new(RuntimeDownloadTask::new(operation.clone()));
    {
      let mut operations = self.operations.lock().await;
      let duplicate_active = operations.values().any(|existing| {
        let existing = existing.snapshot();
        existing.name == operation.name
          && existing.version == operation.version
          && existing.platform == operation.platform
          && existing.architecture == operation.architecture
          && matches!(
            existing.status,
            RuntimeUpdateOperationStatus::Queued | RuntimeUpdateOperationStatus::Downloading
          )
      });
      if duplicate_active {
        bail!("the requested Runtime is already downloading");
      }
      if operations.len() >= MAX_RUNTIME_UPDATE_OPERATIONS {
        operations.retain(|_, existing| {
          matches!(
            existing.snapshot().status,
            RuntimeUpdateOperationStatus::Queued | RuntimeUpdateOperationStatus::Downloading
          )
        });
      }
      if operations.len() >= MAX_RUNTIME_UPDATE_OPERATIONS {
        bail!("too many Runtime downloads are active");
      }
      operations.insert(operation_id, Arc::clone(&task));
    }

    let background_operation = operation.clone();
    tokio::spawn(async move {
      if !task.begin_download() {
        return;
      }
      let progress_task = Arc::clone(&task);
      let cancellation_task = Arc::clone(&task);
      let result = fabdev_updater::download_cached_runtime_update(
        fabdev_updater::RuntimeDownloadRequest {
          cache_directory: &cache_directory,
          current_app_version: env!("CARGO_PKG_VERSION"),
          current_agent_protocol_version: PROTOCOL_VERSION,
          name: &background_operation.name,
          version: &background_operation.version,
          platform: &background_operation.platform,
          architecture: &background_operation.architecture,
        },
        move |downloaded, _| progress_task.set_progress(downloaded),
        move || cancellation_task.is_cancelled(),
      )
      .await;
      if task.is_cancelled() {
        if let Ok(downloaded) = result {
          let _ = tokio::fs::remove_file(downloaded.path).await;
        }
        task.set_status(RuntimeUpdateOperationStatus::Cancelled, None);
      } else {
        match result {
          Ok(_) => {
            task.set_progress(background_operation.total_bytes);
            task.set_status(RuntimeUpdateOperationStatus::Verified, None);
          }
          Err(error) => task.set_status(
            RuntimeUpdateOperationStatus::Failed,
            Some(error.to_string()),
          ),
        }
      }
    });
    Ok(operation)
  }

  async fn get(&self, operation_id: uuid::Uuid) -> Result<RuntimeUpdateOperation> {
    self
      .operations
      .lock()
      .await
      .get(&operation_id)
      .map(|operation| operation.snapshot())
      .context("Runtime update operation was not found")
  }

  async fn cancel(&self, operation_id: uuid::Uuid) -> Result<RuntimeUpdateOperation> {
    let operation = self
      .operations
      .lock()
      .await
      .get(&operation_id)
      .cloned()
      .context("Runtime update operation was not found")?;
    operation.cancel()
  }

  async fn cancel_all(&self) {
    for operation in self.operations.lock().await.values() {
      let _ = operation.cancel();
    }
  }
}

struct LanShareState {
  upstream: SocketAddr,
  server: Option<ShareServer>,
  info: Option<LanShareInfo>,
}

impl LanShareState {
  fn new(http_port: u16) -> Self {
    Self {
      upstream: SocketAddr::from((Ipv4Addr::LOCALHOST, http_port)),
      server: None,
      info: None,
    }
  }

  fn info(&self) -> Option<LanShareInfo> {
    self.info.clone()
  }

  fn contains(&self, site_id: uuid::Uuid) -> bool {
    self
      .info
      .as_ref()
      .is_some_and(|info| info.sites.iter().any(|site| site.site_id == site_id))
  }

  async fn start(&mut self, site: &Site, port: u16) -> Result<LanShareInfo> {
    if port < 1024 {
      bail!("LAN Site Share must use an unprivileged port");
    }
    if let Some(info) = self.info.as_mut() {
      if info.port != port {
        bail!(
          "LAN Site Share is already running on port {}; stop all shared Sites before changing the port",
          info.port
        );
      }
      if !info.sites.iter().any(|shared| shared.site_id == site.id) {
        info.sites.push(LanShareSiteInfo {
          site_id: site.id,
          domain: site.domain.clone(),
        });
        info
          .sites
          .sort_by(|left, right| left.domain.cmp(&right.domain));
      }
      let info = info.clone();
      if let Some(server) = self.server.as_ref() {
        server
          .set_allowed_domains(info.sites.iter().map(|site| site.domain.clone()).collect())
          .await?;
      }
      return Ok(info);
    }
    let server = ShareServer::start_restricted(
      SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
      self.upstream,
      vec![site.domain.clone()],
    )
    .await?;
    let info = LanShareInfo {
      host: discover_lan_ipv4()?.to_string(),
      port: server.local_addr().port(),
      sites: vec![LanShareSiteInfo {
        site_id: site.id,
        domain: site.domain.clone(),
      }],
    };
    self.server = Some(server);
    self.info = Some(info.clone());
    Ok(info)
  }

  async fn stop_site(&mut self, site_id: uuid::Uuid) -> Result<Option<LanShareInfo>> {
    let Some(info) = self.info.as_mut() else {
      return Ok(None);
    };
    info.sites.retain(|site| site.site_id != site_id);
    if info.sites.is_empty() {
      self.stop().await?;
      return Ok(None);
    }
    let info = self.info.clone();
    if let (Some(server), Some(info)) = (self.server.as_ref(), info.as_ref()) {
      server
        .set_allowed_domains(info.sites.iter().map(|site| site.domain.clone()).collect())
        .await?;
    }
    Ok(info)
  }

  async fn update_site(&mut self, site: &Site) -> Result<()> {
    let Some(info) = self.info.as_mut() else {
      return Ok(());
    };
    let Some(shared) = info
      .sites
      .iter_mut()
      .find(|shared| shared.site_id == site.id)
    else {
      return Ok(());
    };
    if shared.domain == site.domain {
      return Ok(());
    }
    let previous_domain = std::mem::replace(&mut shared.domain, site.domain.clone());
    info
      .sites
      .sort_by(|left, right| left.domain.cmp(&right.domain));
    if let Some(server) = self.server.as_ref() {
      if let Err(error) = server
        .set_allowed_domains(info.sites.iter().map(|site| site.domain.clone()).collect())
        .await
      {
        if let Some(shared) = info
          .sites
          .iter_mut()
          .find(|shared| shared.site_id == site.id)
        {
          shared.domain = previous_domain;
        }
        info
          .sites
          .sort_by(|left, right| left.domain.cmp(&right.domain));
        return Err(error);
      }
    }
    Ok(())
  }

  async fn stop(&mut self) -> Result<()> {
    if let Some(mut server) = self.server.take() {
      server.stop().await?;
    }
    self.info = None;
    Ok(())
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  let arguments = Arguments::parse();
  let paths = match &arguments.data_dir {
    Some(path) => AppPaths::from_root(path),
    None => AppPaths::discover().context("unable to locate fabDev application data")?,
  };
  paths
    .ensure()
    .context("unable to create fabDev application directories")?;
  fabdev_updater::cleanup_runtime_update_partials(&paths.cache)
    .await
    .context("unable to clean stale Runtime update partial files")?;
  let endpoint = default_agent_endpoint(&arguments, &paths);

  let repository =
    SiteRepository::open(paths.database()).context("unable to open site database")?;
  let proxy_running_ids = repository
    .proxy_running_ids()
    .context("unable to load Proxy Manager state")?;
  let proxy_connections = repository
    .proxy_connections()
    .context("unable to load Proxy Manager connections")?
    .unwrap_or_default();
  let mut default_runtimes = RuntimePaths::from_runtime_root(&paths.runtimes);
  default_runtimes.mariadb = active_mariadb_runtime_path(&paths.runtimes)?;
  let runtimes = RuntimePaths {
    dnsmasq: arguments
      .dnsmasq_runtime
      .unwrap_or(default_runtimes.dnsmasq),
    nginx: arguments.nginx_runtime.unwrap_or(default_runtimes.nginx),
    php: arguments.php_runtime.unwrap_or(default_runtimes.php),
    mariadb: arguments
      .mariadb_runtime
      .unwrap_or(default_runtimes.mariadb),
  };
  let supervisor = ServiceSupervisor::new(
    paths.clone(),
    runtimes,
    ServicePorts {
      dns: arguments.dns_port,
      http: arguments.http_port,
      https: arguments.https_port,
      mariadb: arguments.mariadb_port,
    },
  );
  let proxy_manager =
    ProxyManager::new(proxy_connections).context("unable to initialize Proxy Manager")?;
  let state = Arc::new(AgentState {
    paths,
    sites: Mutex::new(repository),
    services: Mutex::new(supervisor),
    lan_share: Mutex::new(LanShareState::new(arguments.http_port)),
    proxy_manager: Mutex::new(proxy_manager),
    runtime_updates: RuntimeUpdateManager::default(),
    shutdown: Arc::new(Notify::new()),
  });
  {
    let mut manager = state.proxy_manager.lock().await;
    for connection_id in proxy_running_ids {
      if let Err(error) = manager.start(&connection_id).await {
        eprintln!("unable to restore Proxy connection {connection_id}: {error:#}");
      }
    }
  }
  serve(endpoint, state).await
}

#[cfg(unix)]
fn default_agent_endpoint(arguments: &Arguments, paths: &AppPaths) -> AgentEndpoint {
  AgentEndpoint::UnixSocket(
    arguments
      .socket
      .clone()
      .unwrap_or_else(|| paths.agent_socket()),
  )
}

#[cfg(windows)]
fn default_agent_endpoint(arguments: &Arguments, paths: &AppPaths) -> AgentEndpoint {
  arguments
    .pipe
    .clone()
    .map(AgentEndpoint::NamedPipe)
    .unwrap_or_else(|| paths.agent_endpoint())
}

#[cfg(unix)]
async fn prepare_socket(path: &PathBuf) -> Result<()> {
  if let Some(parent) = path.parent() {
    tokio::fs::create_dir_all(parent).await?;
  }
  if !path.exists() {
    return Ok(());
  }
  if UnixStream::connect(path).await.is_ok() {
    bail!(
      "another fabDev Agent is already listening at {}",
      path.display()
    );
  }
  tokio::fs::remove_file(path)
    .await
    .with_context(|| format!("unable to remove stale socket at {}", path.display()))?;
  Ok(())
}

#[cfg(unix)]
struct AgentSocketGuard(PathBuf);

#[cfg(unix)]
impl Drop for AgentSocketGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.0);
  }
}

#[cfg(unix)]
async fn serve(endpoint: AgentEndpoint, state: Arc<AgentState>) -> Result<()> {
  let AgentEndpoint::UnixSocket(socket_path) = endpoint;
  prepare_socket(&socket_path).await?;
  let listener = UnixListener::bind(&socket_path)
    .with_context(|| format!("unable to bind agent socket at {}", socket_path.display()))?;
  let _socket_guard = AgentSocketGuard(socket_path.clone());
  println!(
    "fabDev Agent {} listening at {}",
    env!("CARGO_PKG_VERSION"),
    socket_path.display()
  );

  loop {
    tokio::select! {
      accepted = listener.accept() => {
        let (stream, _) = accepted?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
          if let Err(error) = handle_connection(stream, state).await {
            eprintln!("agent connection failed: {error:#}");
          }
        });
      }
      () = state.shutdown.notified() => return Ok(()),
    }
  }
}

#[cfg(windows)]
async fn serve(endpoint: AgentEndpoint, state: Arc<AgentState>) -> Result<()> {
  let AgentEndpoint::NamedPipe(pipe_name) = endpoint;
  let mut server = ServerOptions::new()
    .first_pipe_instance(true)
    .create(&pipe_name)
    .with_context(|| format!("unable to create Agent named pipe {pipe_name}"))?;
  println!(
    "fabDev Agent {} listening at {}",
    env!("CARGO_PKG_VERSION"),
    pipe_name
  );

  loop {
    tokio::select! {
      connected = server.connect() => {
        connected?;
        let connected_client = server;
        server = ServerOptions::new().create(&pipe_name)?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
          if let Err(error) = handle_connection(connected_client, state).await {
            eprintln!("agent connection failed: {error:#}");
          }
        });
      }
      () = state.shutdown.notified() => return Ok(()),
    }
  }
}

async fn handle_connection<S>(stream: S, state: Arc<AgentState>) -> Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin,
{
  let (reader, mut writer) = tokio::io::split(stream);
  let mut lines = BufReader::new(reader).lines();
  while let Some(line) = lines.next_line().await? {
    let response = match serde_json::from_str::<AgentRequest>(&line) {
      Ok(request) => handle_request(request, &state).await,
      Err(error) => AgentResponse::Error {
        code: "invalid_request".to_owned(),
        message: error.to_string(),
      },
    };
    writer
      .write_all(serde_json::to_string(&response)?.as_bytes())
      .await?;
    writer.write_all(b"\n").await?;
  }
  Ok(())
}

async fn handle_request(request: AgentRequest, state: &AgentState) -> AgentResponse {
  match request {
    AgentRequest::Ping => AgentResponse::Pong {
      protocol_version: PROTOCOL_VERSION,
    },
    AgentRequest::GetStatus => {
      let sites = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => {
          eprintln!("unable to list Sites for PHP-FPM status: {error}");
          Vec::new()
        }
      };
      let mut services = state.services.lock().await;
      if let Err(error) = services.rotate_logs_if_due() {
        eprintln!("unable to rotate managed logs: {error:#}");
      }
      let mut status = services.status();
      if status.php_fpm == ServiceState::Running {
        status.php_fpm_pools = services.php_fpm_pool_statuses(&sites).await;
      }
      AgentResponse::Status(status)
    }
    AgentRequest::ListSites => match sync_home_sites(state).await {
      Ok(sites) => AgentResponse::Sites(sites),
      Err(error) => internal_error(error),
    },
    AgentRequest::GetSiteHome => match load_site_home_settings(state).await {
      Ok(settings) => AgentResponse::SiteHomeSettings(settings),
      Err(error) => AgentResponse::Error {
        code: "site_home_read_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::SaveSiteHome(settings) => match save_site_home(state, &settings.path).await {
      Ok(settings) => AgentResponse::SiteHomeSettings(settings),
      Err(error) => AgentResponse::Error {
        code: "site_home_save_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::AddSite(input) => {
      let site = match create_site(input) {
        Ok(site) => site,
        Err(error) => {
          return AgentResponse::Error {
            code: "invalid_site".to_owned(),
            message: error.to_string(),
          };
        }
      };
      if let Err(error) = state.sites.lock().await.insert(&site) {
        return internal_error(error);
      }
      let sites_after_add = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => {
          let _ = state.sites.lock().await.remove(&site.id);
          return internal_error(error);
        }
      };
      let result = {
        let mut services = state.services.lock().await;
        match services.sync_site_domains(&sites_after_add).await {
          Ok(()) => services.add_site_config(&site).await,
          Err(error) => Err(error),
        }
      };
      if let Err(error) = result {
        let rollback = state.sites.lock().await.remove(&site.id);
        if rollback.as_ref().is_ok_and(|site| site.is_some()) {
          let remaining_sites = state.sites.lock().await.list().unwrap_or_default();
          let _ = state
            .services
            .lock()
            .await
            .sync_site_domains(&remaining_sites)
            .await;
        }
        let message = match rollback {
          Ok(Some(_)) => error.to_string(),
          Ok(None) => format!("{error}; Site registry rollback found no matching entry"),
          Err(rollback_error) => {
            format!("{error}; unable to rollback Site registry entry: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "internal_error".to_owned(),
          message,
        };
      }
      AgentResponse::SiteAdded(site)
    }
    AgentRequest::UpdateSite { site_id, input } => {
      match state.sites.lock().await.is_home_site(&site_id) {
        Ok(true) => {
          return AgentResponse::Error {
            code: "home_site_managed".to_owned(),
            message: "Site Home projects are managed by their folder names".to_owned(),
          };
        }
        Ok(false) => {}
        Err(error) => return internal_error(error),
      }
      let previous = match state
        .sites
        .lock()
        .await
        .list()
        .map(|sites| sites.into_iter().find(|site| site.id == site_id))
      {
        Ok(Some(site)) => site,
        Ok(None) => {
          return AgentResponse::Error {
            code: "site_not_found".to_owned(),
            message: "Site not found".to_owned(),
          };
        }
        Err(error) => return internal_error(error),
      };
      let updated = match edit_site(&previous, input) {
        Ok(site) => site,
        Err(error) => {
          return AgentResponse::Error {
            code: "invalid_site".to_owned(),
            message: error.to_string(),
          };
        }
      };
      if previous == updated {
        return AgentResponse::SiteUpdated(updated);
      }
      if let Err(error) = state.sites.lock().await.update_site(&updated) {
        return AgentResponse::Error {
          code: "site_update_failed".to_owned(),
          message: error.to_string(),
        };
      }
      let sites_after_update = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => {
          let _ = state.sites.lock().await.update_site(&previous);
          return internal_error(error);
        }
      };
      let config_result = {
        let mut services = state.services.lock().await;
        match services.sync_site_domains(&sites_after_update).await {
          Ok(()) => services.update_site_config(&previous, &updated).await,
          Err(error) => Err(error),
        }
      };
      if let Err(error) = config_result {
        let rollback = state.sites.lock().await.update_site(&previous);
        if rollback.as_ref().is_ok_and(|change| change.is_some()) {
          let restored_sites = state.sites.lock().await.list().unwrap_or_default();
          let _ = state
            .services
            .lock()
            .await
            .sync_site_domains(&restored_sites)
            .await;
        }
        let message = match rollback {
          Ok(Some(_)) => error.to_string(),
          Ok(None) => format!("{error}; Site registry rollback found no matching entry"),
          Err(rollback_error) => {
            format!("{error}; unable to rollback Site registry entry: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "site_update_failed".to_owned(),
          message,
        };
      }
      if let Err(error) = state.lan_share.lock().await.update_site(&updated).await {
        let rollback = state.sites.lock().await.update_site(&previous);
        if rollback.as_ref().is_ok_and(|change| change.is_some()) {
          let restored_sites = state.sites.lock().await.list().unwrap_or_default();
          let mut services = state.services.lock().await;
          let _ = services.sync_site_domains(&restored_sites).await;
          let _ = services.update_site_config(&updated, &previous).await;
        }
        let message = match rollback {
          Ok(Some(_)) => error.to_string(),
          Ok(None) => format!("{error}; Site registry rollback found no matching entry"),
          Err(rollback_error) => {
            format!("{error}; unable to rollback Site registry entry: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "site_update_failed".to_owned(),
          message,
        };
      }
      AgentResponse::SiteUpdated(updated)
    }
    AgentRequest::RemoveSite { site_id } => {
      match state.sites.lock().await.is_home_site(&site_id) {
        Ok(true) => {
          return AgentResponse::Error {
            code: "home_site_managed".to_owned(),
            message: "remove the project folder from the Site Home directory instead".to_owned(),
          };
        }
        Ok(false) => {}
        Err(error) => return internal_error(error),
      }
      let is_shared = state.lan_share.lock().await.contains(site_id);
      if is_shared {
        if let Err(error) = state.lan_share.lock().await.stop_site(site_id).await {
          return AgentResponse::Error {
            code: "lan_share_stop_failed".to_owned(),
            message: error.to_string(),
          };
        }
      }
      let site = match state.sites.lock().await.remove(&site_id) {
        Ok(Some(site)) => site,
        Ok(None) => {
          return AgentResponse::Error {
            code: "site_not_found".to_owned(),
            message: "Site not found".to_owned(),
          };
        }
        Err(error) => return internal_error(error),
      };
      let remaining_sites = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => {
          let _ = state.sites.lock().await.insert(&site);
          return internal_error(error);
        }
      };
      if let Err(error) = state
        .services
        .lock()
        .await
        .sync_site_domains(&remaining_sites)
        .await
      {
        let _ = state.sites.lock().await.insert(&site);
        return internal_error(error);
      }
      if let Err(error) = state
        .services
        .lock()
        .await
        .remove_site_config(&site, &remaining_sites)
        .await
      {
        let rollback = state.sites.lock().await.insert(&site);
        if rollback.is_ok() {
          let restored_sites = state.sites.lock().await.list().unwrap_or_default();
          let _ = state
            .services
            .lock()
            .await
            .sync_site_domains(&restored_sites)
            .await;
        }
        let message = match rollback {
          Ok(()) => error.to_string(),
          Err(rollback_error) => {
            format!("{error}; unable to restore Site registry entry: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "internal_error".to_owned(),
          message,
        };
      }
      AgentResponse::SiteRemoved(site)
    }
    AgentRequest::SetSitePhp {
      site_id,
      php_version,
    } => {
      let (previous, updated) = match state
        .sites
        .lock()
        .await
        .update_php_version(&site_id, php_version.as_ref())
      {
        Ok(Some(change)) => change,
        Ok(None) => {
          return AgentResponse::Error {
            code: "site_not_found".to_owned(),
            message: "Site not found".to_owned(),
          };
        }
        Err(error) => return internal_error(error),
      };
      let sites_after_update = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => {
          let _ = state
            .sites
            .lock()
            .await
            .update_php_version(&site_id, previous.php_version.as_ref());
          return internal_error(error);
        }
      };
      if let Err(error) = state
        .services
        .lock()
        .await
        .update_site_php_config(&previous, &updated, &sites_after_update)
        .await
      {
        let rollback = state
          .sites
          .lock()
          .await
          .update_php_version(&site_id, previous.php_version.as_ref());
        let message = match rollback {
          Ok(Some(_)) => error.to_string(),
          Ok(None) => format!("{error}; Site registry rollback found no matching entry"),
          Err(rollback_error) => {
            format!("{error}; unable to rollback Site PHP version: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "site_php_switch_failed".to_owned(),
          message,
        };
      }
      AgentResponse::SitePhpChanged(updated)
    }
    AgentRequest::EnsureLocalCa => match ensure_local_ca(&state.paths) {
      Ok(info) => AgentResponse::LocalCaReady(info),
      Err(error) => AgentResponse::Error {
        code: "local_ca_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::SetSiteHttps { site_id, secured } => {
      if secured {
        if let Err(error) = ensure_local_ca(&state.paths) {
          return AgentResponse::Error {
            code: "local_ca_failed".to_owned(),
            message: error.to_string(),
          };
        }
      }
      let (previous, updated) = match state.sites.lock().await.update_https(&site_id, secured) {
        Ok(Some(change)) => change,
        Ok(None) => {
          return AgentResponse::Error {
            code: "site_not_found".to_owned(),
            message: "Site not found".to_owned(),
          };
        }
        Err(error) => return internal_error(error),
      };
      if let Err(error) = state
        .services
        .lock()
        .await
        .update_site_https_config(&previous, &updated)
        .await
      {
        let rollback = state
          .sites
          .lock()
          .await
          .update_https(&site_id, previous.secured);
        let message = match rollback {
          Ok(Some(_)) => error.to_string(),
          Ok(None) => format!("{error}; Site registry rollback found no matching entry"),
          Err(rollback_error) => {
            format!("{error}; unable to rollback Site HTTPS setting: {rollback_error}")
          }
        };
        return AgentResponse::Error {
          code: "site_https_change_failed".to_owned(),
          message,
        };
      }
      AgentResponse::SiteHttpsChanged(updated)
    }
    AgentRequest::GetLanShare => AgentResponse::LanShare(state.lan_share.lock().await.info()),
    AgentRequest::StartLanShare { site_id, port } => {
      let site = match state.sites.lock().await.list() {
        Ok(sites) => sites.into_iter().find(|site| site.id == site_id),
        Err(error) => return internal_error(error),
      };
      let Some(site) = site else {
        return AgentResponse::Error {
          code: "site_not_found".to_owned(),
          message: "Site not found".to_owned(),
        };
      };
      if !site.enabled {
        return AgentResponse::Error {
          code: "site_disabled".to_owned(),
          message: "enable the Site before sharing it over LAN".to_owned(),
        };
      }
      if state.services.lock().await.status().nginx != ServiceState::Running {
        return AgentResponse::Error {
          code: "nginx_not_running".to_owned(),
          message: "start fabDev Web services before sharing a Site".to_owned(),
        };
      }
      match state.lan_share.lock().await.start(&site, port).await {
        Ok(info) => AgentResponse::LanShare(Some(info)),
        Err(error) => AgentResponse::Error {
          code: "lan_share_start_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::StopLanShareSite { site_id } => {
      match state.lan_share.lock().await.stop_site(site_id).await {
        Ok(info) => AgentResponse::LanShare(info),
        Err(error) => AgentResponse::Error {
          code: "lan_share_stop_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::StopLanShare => match state.lan_share.lock().await.stop().await {
      Ok(()) => AgentResponse::LanShare(None),
      Err(error) => AgentResponse::Error {
        code: "lan_share_stop_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::CheckRuntimeUpdates => match check_runtime_updates(state).await {
      Ok(check) => AgentResponse::RuntimeUpdates(check),
      Err(error) => AgentResponse::Error {
        code: "runtime_update_check_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::StartRuntimeDownload { name, version } => {
      let artifact = match cached_runtime_update_artifact(state, &name, &version).await {
        Ok(artifact) => artifact,
        Err(error) => {
          return AgentResponse::Error {
            code: "runtime_download_invalid".to_owned(),
            message: error.to_string(),
          };
        }
      };
      match state
        .runtime_updates
        .start(state.paths.cache.clone(), artifact)
        .await
      {
        Ok(operation) => AgentResponse::RuntimeUpdateOperation(operation),
        Err(error) => AgentResponse::Error {
          code: "runtime_download_start_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetRuntimeUpdateOperation { operation_id } => {
      match state.runtime_updates.get(operation_id).await {
        Ok(operation) => AgentResponse::RuntimeUpdateOperation(operation),
        Err(error) => AgentResponse::Error {
          code: "runtime_update_operation_not_found".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::CancelRuntimeDownload { operation_id } => {
      match state.runtime_updates.cancel(operation_id).await {
        Ok(operation) => AgentResponse::RuntimeUpdateOperation(operation),
        Err(error) => AgentResponse::Error {
          code: "runtime_download_cancel_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::InstallDownloadedRuntime { operation_id } => {
      match state.runtime_updates.get(operation_id).await {
        Ok(_) => AgentResponse::Error {
          code: "runtime_install_not_available".to_owned(),
          message: "online Runtime installation is reserved for P2.3".to_owned(),
        },
        Err(error) => AgentResponse::Error {
          code: "runtime_update_operation_not_found".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::ListPhpRuntimes => match php_runtime_state(state).await {
      Ok(runtime_state) => AgentResponse::PhpRuntimes(runtime_state),
      Err(error) => internal_error(error),
    },
    AgentRequest::InstallPhpRuntime {
      artifact_path,
      release_path,
    } => {
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<PhpVersion> {
        let release: RuntimeRelease =
          serde_json::from_reader(std::fs::File::open(&release_path).with_context(|| {
            format!(
              "unable to open Runtime release descriptor: {}",
              release_path.display()
            )
          })?)
          .context("invalid Runtime release descriptor")?;
        validate_php_release(&release, &artifact_path)?;
        let activate = active_version(&runtime_root, "php")?.is_none();
        install_tar_gz_with_activation(
          artifact_path,
          &release.sha256,
          &release.name,
          &release.version,
          runtime_root,
          activate,
        )?;
        php_series(&release.version)?
          .parse::<PhpVersion>()
          .context("invalid installed PHP Runtime series")
      })
      .await;
      match result {
        Ok(Ok(php_version)) => {
          if let Err(error) = state
            .services
            .lock()
            .await
            .initialize_empty_php_ini(&php_version)
          {
            return AgentResponse::Error {
              code: "php_ini_initialize_failed".to_owned(),
              message: error.to_string(),
            };
          }
          match php_runtime_state(state).await {
            Ok(runtime_state) => AgentResponse::PhpRuntimeInstalled(runtime_state),
            Err(error) => internal_error(error),
          }
        }
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_install_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::SetGlobalPhp { version } => {
      let runtime_root = state.paths.runtimes.clone();
      let result =
        tokio::task::spawn_blocking(move || set_active_version(runtime_root, "php", &version))
          .await;
      match result {
        Ok(Ok(())) => match php_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::GlobalPhpChanged(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_switch_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::RemovePhpRuntime { version } => {
      let series = match php_series(&version) {
        Ok(series) => series,
        Err(error) => {
          return AgentResponse::Error {
            code: "invalid_runtime_version".to_owned(),
            message: error.to_string(),
          };
        }
      };
      let sites = match state.sites.lock().await.list() {
        Ok(sites) => sites,
        Err(error) => return internal_error(error),
      };
      let used_by = sites
        .iter()
        .filter(|site| {
          site
            .php_version
            .as_ref()
            .is_some_and(|version| version.to_string() == series)
        })
        .map(|site| site.domain.clone())
        .collect::<Vec<_>>();
      if !used_by.is_empty() {
        return AgentResponse::Error {
          code: "runtime_in_use".to_owned(),
          message: format!("PHP {version} is used by Sites: {}", used_by.join(", ")),
        };
      }
      if let Err(error) = state.services.lock().await.ensure_default_php_ini() {
        return AgentResponse::Error {
          code: "php_ini_default_failed".to_owned(),
          message: error.to_string(),
        };
      }
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        mark_runtime_removed(&runtime_root, "php", &version)?;
        if let Err(error) = remove_installed_version(&runtime_root, "php", &version) {
          let _ = fabdev_runtime::clear_runtime_removal_marker(&runtime_root, "php", &version);
          return Err(error.into());
        }
        Ok(())
      })
      .await;
      match result {
        Ok(Ok(())) => match php_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::PhpRuntimeRemoved(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_remove_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::GetPhpIni { php_version } => {
      match state.services.lock().await.read_php_ini(&php_version) {
        Ok(contents) => AgentResponse::PhpIni {
          php_version,
          contents,
        },
        Err(error) => AgentResponse::Error {
          code: "php_ini_read_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::SavePhpIni {
      php_version,
      contents,
    } => {
      match state
        .services
        .lock()
        .await
        .save_php_ini(&php_version, &contents)
        .await
      {
        Ok(()) => AgentResponse::PhpIniSaved { php_version },
        Err(error) => AgentResponse::Error {
          code: "php_ini_save_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetDefaultPhpIni => match state.services.lock().await.read_default_php_ini() {
      Ok(contents) => AgentResponse::DefaultPhpIni { contents },
      Err(error) => AgentResponse::Error {
        code: "php_ini_read_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::SaveDefaultPhpIni { contents } => {
      match state.services.lock().await.save_default_php_ini(&contents) {
        Ok(()) => AgentResponse::DefaultPhpIniSaved,
        Err(error) => AgentResponse::Error {
          code: "php_ini_save_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetErpPhpIni { php_version } => {
      match state
        .services
        .lock()
        .await
        .read_erp_php_ini(php_version.as_ref())
      {
        Ok(contents) => AgentResponse::ErpPhpIni {
          php_version,
          contents,
        },
        Err(error) => AgentResponse::Error {
          code: "php_ini_read_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetNodeRuntime => match node_runtime_state(state).await {
      Ok(runtime_state) => AgentResponse::NodeRuntime(runtime_state),
      Err(error) => internal_error(error),
    },
    AgentRequest::InstallNodeRuntime {
      artifact_path,
      release_path,
    } => {
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        let release: RuntimeRelease =
          serde_json::from_reader(std::fs::File::open(&release_path).with_context(|| {
            format!(
              "unable to open Runtime release descriptor: {}",
              release_path.display()
            )
          })?)
          .context("invalid Runtime release descriptor")?;
        validate_node_release(&release, &artifact_path)?;
        install_tar_gz_with_activation(
          artifact_path,
          &release.sha256,
          &release.name,
          &release.version,
          runtime_root,
          true,
        )?;
        Ok(())
      })
      .await;
      match result {
        Ok(Ok(())) => match node_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::NodeRuntimeInstalled(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_install_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::RemoveNodeRuntime => {
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        let version = deactivate_runtime(&runtime_root, "node")?
          .context("fabDev Node.js Runtime is not installed")?;
        if version != STABLE_NODE_VERSION {
          set_active_version(&runtime_root, "node", &version)?;
          bail!("unsupported installed Node.js Runtime version: {version}");
        }
        if let Err(error) = remove_installed_version(&runtime_root, "node", &version) {
          set_active_version(&runtime_root, "node", &version).with_context(|| {
            format!("unable to restore Node.js Runtime {version} after removal failed")
          })?;
          return Err(error.into());
        }
        Ok(())
      })
      .await;
      match result {
        Ok(Ok(())) => match node_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::NodeRuntimeRemoved(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_remove_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::GetProxyManager => {
      AgentResponse::ProxyManager(state.proxy_manager.lock().await.state().await)
    }
    AgentRequest::AddProxyConnection(input) => {
      let mut manager = state.proxy_manager.lock().await;
      match manager.add(input) {
        Ok(connection_id) => {
          if let Err(error) = state
            .sites
            .lock()
            .await
            .save_proxy_connections(&manager.connections())
          {
            let _ = manager.remove(&connection_id).await;
            return AgentResponse::Error {
              code: "proxy_save_failed".to_owned(),
              message: error.to_string(),
            };
          }
          AgentResponse::ProxyManager(manager.state().await)
        }
        Err(error) => AgentResponse::Error {
          code: "proxy_add_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::UpdateProxyConnection {
      connection_id,
      input,
    } => {
      let mut manager = state.proxy_manager.lock().await;
      match manager.update(&connection_id, input).await {
        Ok((previous, was_running)) => {
          let persistence_result = {
            let repository = state.sites.lock().await;
            repository
              .save_proxy_connections(&manager.connections())
              .and_then(|()| repository.save_proxy_running_ids(&manager.running_ids()))
          };
          if let Err(error) = persistence_result {
            let _ = manager.restore_update(previous, was_running).await;
            let repository = state.sites.lock().await;
            let _ = repository.save_proxy_connections(&manager.connections());
            let _ = repository.save_proxy_running_ids(&manager.running_ids());
            return AgentResponse::Error {
              code: "proxy_save_failed".to_owned(),
              message: error.to_string(),
            };
          }
          AgentResponse::ProxyManager(manager.state().await)
        }
        Err(error) => AgentResponse::Error {
          code: "proxy_update_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::RemoveProxyConnection { connection_id } => {
      let mut manager = state.proxy_manager.lock().await;
      let was_running = manager.running_ids().contains(&connection_id);
      match manager.remove(&connection_id).await {
        Ok(settings) => {
          let persistence_result = {
            let repository = state.sites.lock().await;
            repository
              .save_proxy_connections(&manager.connections())
              .and_then(|()| repository.save_proxy_running_ids(&manager.running_ids()))
          };
          if let Err(error) = persistence_result {
            let _ = manager.restore(settings);
            if was_running {
              let _ = manager.start(&connection_id).await;
            }
            let repository = state.sites.lock().await;
            let _ = repository.save_proxy_connections(&manager.connections());
            let _ = repository.save_proxy_running_ids(&manager.running_ids());
            return AgentResponse::Error {
              code: "proxy_save_failed".to_owned(),
              message: error.to_string(),
            };
          }
          AgentResponse::ProxyManager(manager.state().await)
        }
        Err(error) => AgentResponse::Error {
          code: "proxy_remove_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::StartProxyConnection { connection_id } => {
      let mut manager = state.proxy_manager.lock().await;
      match manager.start(&connection_id).await {
        Ok(()) => {
          if let Err(error) = state
            .sites
            .lock()
            .await
            .save_proxy_running_ids(&manager.running_ids())
          {
            return AgentResponse::Error {
              code: "proxy_state_save_failed".to_owned(),
              message: error.to_string(),
            };
          }
          AgentResponse::ProxyManager(manager.state().await)
        }
        Err(error) => AgentResponse::Error {
          code: "proxy_start_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::StopProxyConnection { connection_id } => {
      let mut manager = state.proxy_manager.lock().await;
      match manager.stop(&connection_id).await {
        Ok(()) => {
          if let Err(error) = state
            .sites
            .lock()
            .await
            .save_proxy_running_ids(&manager.running_ids())
          {
            return AgentResponse::Error {
              code: "proxy_state_save_failed".to_owned(),
              message: error.to_string(),
            };
          }
          AgentResponse::ProxyManager(manager.state().await)
        }
        Err(error) => AgentResponse::Error {
          code: "proxy_stop_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::StartAllProxyConnections => {
      let mut manager = state.proxy_manager.lock().await;
      let proxy_state = manager.start_all().await;
      if let Err(error) = state
        .sites
        .lock()
        .await
        .save_proxy_running_ids(&manager.running_ids())
      {
        return AgentResponse::Error {
          code: "proxy_state_save_failed".to_owned(),
          message: error.to_string(),
        };
      }
      AgentResponse::ProxyManager(proxy_state)
    }
    AgentRequest::StopAllProxyConnections => {
      let proxy_state = state.proxy_manager.lock().await.stop_all().await;
      if let Err(error) = state.sites.lock().await.save_proxy_running_ids(&[]) {
        return AgentResponse::Error {
          code: "proxy_state_save_failed".to_owned(),
          message: error.to_string(),
        };
      }
      AgentResponse::ProxyManager(proxy_state)
    }
    AgentRequest::Shutdown => match stop_for_shutdown(state).await {
      Ok(()) => {
        state.runtime_updates.cancel_all().await;
        let shutdown = Arc::clone(&state.shutdown);
        tokio::spawn(async move {
          tokio::time::sleep(std::time::Duration::from_millis(50)).await;
          shutdown.notify_one();
        });
        AgentResponse::Stopped
      }
      Err(error) => internal_error(error),
    },
    AgentRequest::StartAll => {
      let sites = match sync_home_sites(state).await {
        Ok(sites) => sites,
        Err(error) => return internal_error(error),
      };
      let sites = sites
        .into_iter()
        .filter(|site| site.enabled)
        .collect::<Vec<_>>();
      if sites.is_empty() {
        return AgentResponse::Error {
          code: "site_required".to_owned(),
          message: "add an enabled Site before starting services".to_owned(),
        };
      }
      let requires_php = sites.iter().any(|site| site.php_version.is_some());
      let mut services = state.services.lock().await;
      let status = services.status();
      if web_services_ready(&status, requires_php) {
        return AgentResponse::Started;
      }
      if web_services_need_cleanup(&status) {
        if let Err(error) = services.stop_all().await {
          return internal_error(error);
        }
      }
      match services.start_all(&sites).await {
        Ok(()) => AgentResponse::Started,
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::StopAll => match stop_all(state).await {
      Ok(()) => AgentResponse::Stopped,
      Err(error) => internal_error(error),
    },
    AgentRequest::StartMariaDb => match state
      .services
      .lock()
      .await
      .start_mariadb_and_remember()
      .await
    {
      Ok(()) => AgentResponse::MariaDbStarted,
      Err(error) => AgentResponse::Error {
        code: "mariadb_start_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::StopMariaDb => match state
      .services
      .lock()
      .await
      .stop_mariadb_and_remember()
      .await
    {
      Ok(()) => AgentResponse::MariaDbStopped,
      Err(error) => AgentResponse::Error {
        code: "mariadb_stop_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::RestoreMariaDbLastState => {
      match state
        .services
        .lock()
        .await
        .restore_mariadb_last_state()
        .await
      {
        Ok(()) => AgentResponse::MariaDbStateRestored,
        Err(error) => AgentResponse::Error {
          code: "mariadb_restore_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetMariaDbSettings => match state.services.lock().await.mariadb_settings() {
      Ok(settings) => AgentResponse::MariaDbSettings(settings),
      Err(error) => AgentResponse::Error {
        code: "mariadb_settings_read_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::SaveMariaDbSettings(settings) => {
      match state
        .services
        .lock()
        .await
        .save_mariadb_settings_and_apply(settings)
        .await
      {
        Ok(settings) => AgentResponse::MariaDbSettings(settings),
        Err(error) => AgentResponse::Error {
          code: "mariadb_settings_save_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::GetMariaDbConfig => match state.services.lock().await.read_mariadb_config() {
      Ok((filename, contents)) => AgentResponse::MariaDbConfig { filename, contents },
      Err(error) => AgentResponse::Error {
        code: "mariadb_config_read_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::SaveMariaDbConfig { contents } => {
      match state.services.lock().await.save_mariadb_config(&contents) {
        Ok((filename, contents)) => AgentResponse::MariaDbConfigSaved { filename, contents },
        Err(error) => AgentResponse::Error {
          code: "mariadb_config_save_failed".to_owned(),
          message: error.to_string(),
        },
      }
    }
    AgentRequest::SetMariaDbRootPassword {
      current_password,
      new_password,
    } => match state
      .services
      .lock()
      .await
      .set_mariadb_root_password(&current_password, &new_password)
      .await
    {
      Ok(()) => AgentResponse::MariaDbRootPasswordChanged,
      Err(error) => AgentResponse::Error {
        code: "mariadb_root_password_change_failed".to_owned(),
        message: error.to_string(),
      },
    },
    AgentRequest::InstallMariaDbRuntime {
      artifact_path,
      release_path,
    } => {
      if matches!(
        state.services.lock().await.status().mariadb,
        fabdev_core::ServiceState::Running
      ) {
        return AgentResponse::Error {
          code: "runtime_in_use".to_owned(),
          message: "stop MariaDB before installing its Runtime".to_owned(),
        };
      }
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<String> {
        let release: RuntimeRelease =
          serde_json::from_reader(std::fs::File::open(&release_path).with_context(|| {
            format!(
              "unable to open Runtime release descriptor: {}",
              release_path.display()
            )
          })?)
          .context("invalid Runtime release descriptor")?;
        validate_mariadb_release(&release, &artifact_path)?;
        install_tar_gz_with_activation(
          artifact_path,
          &release.sha256,
          &release.name,
          &release.version,
          runtime_root,
          true,
        )?;
        Ok(release.version)
      })
      .await;
      match result {
        Ok(Ok(version)) => {
          let runtime = state.paths.runtimes.join("mariadb").join(&version);
          let mut services = state.services.lock().await;
          services.set_mariadb_runtime(runtime);
          match services.refresh_php_mariadb_connection().await {
            Ok(()) => AgentResponse::MariaDbRuntimeInstalled { version },
            Err(error) => AgentResponse::Error {
              code: "mariadb_connection_apply_failed".to_owned(),
              message: error.to_string(),
            },
          }
        }
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_install_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::RemoveMariaDbRuntime => {
      if matches!(
        state.services.lock().await.status().mariadb,
        fabdev_core::ServiceState::Running
      ) {
        return AgentResponse::Error {
          code: "runtime_in_use".to_owned(),
          message: "stop MariaDB before removing its Runtime".to_owned(),
        };
      }
      if let Err(error) = state.services.lock().await.remember_mariadb_stopped() {
        return AgentResponse::Error {
          code: "mariadb_state_save_failed".to_owned(),
          message: error.to_string(),
        };
      }
      let runtime_root = state.paths.runtimes.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<String> {
        let version = deactivate_runtime(&runtime_root, "mariadb")?
          .context("fabDev MariaDB Runtime is not installed")?;
        if let Err(error) = remove_installed_version(&runtime_root, "mariadb", &version) {
          set_active_version(&runtime_root, "mariadb", &version).with_context(|| {
            format!("unable to restore MariaDB Runtime {version} after removal failed")
          })?;
          return Err(error.into());
        }
        Ok(version)
      })
      .await;
      match result {
        Ok(Ok(version)) => {
          let mut services = state.services.lock().await;
          services.set_mariadb_runtime(state.paths.runtimes.join("mariadb/current"));
          match services.refresh_php_mariadb_connection().await {
            Ok(()) => AgentResponse::MariaDbRuntimeRemoved { version },
            Err(error) => AgentResponse::Error {
              code: "mariadb_connection_apply_failed".to_owned(),
              message: error.to_string(),
            },
          }
        }
        Ok(Err(error)) => AgentResponse::Error {
          code: "runtime_remove_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
  }
}

async fn check_runtime_updates(state: &AgentState) -> Result<RuntimeUpdateCheck> {
  let catalog = fabdev_updater::check_for_runtime_updates(
    &state.paths.cache,
    env!("CARGO_PKG_VERSION"),
    PROTOCOL_VERSION,
  )
  .await?;
  build_runtime_update_check(&catalog, &state.paths.runtimes)
}

async fn cached_runtime_update_artifact(
  state: &AgentState,
  name: &str,
  version: &str,
) -> Result<RuntimeUpdateArtifact> {
  let catalog = fabdev_updater::cached_runtime_catalog(
    &state.paths.cache,
    env!("CARGO_PKG_VERSION"),
    PROTOCOL_VERSION,
  )
  .await?;
  let artifact = runtime_update_artifacts(&catalog, &state.paths.runtimes)?
    .into_iter()
    .find(|artifact| artifact.name == name && artifact.version == version)
    .context("the verified Runtime Catalog does not contain the requested Runtime")?;
  if artifact.installed {
    bail!(
      "Runtime {} {} is already installed",
      artifact.name,
      artifact.version
    );
  }
  Ok(artifact)
}

fn build_runtime_update_check(
  catalog: &ValidatedRuntimeCatalog,
  runtime_root: &Path,
) -> Result<RuntimeUpdateCheck> {
  Ok(RuntimeUpdateCheck {
    catalog_sequence: catalog.catalog.catalog_sequence,
    generated_at: catalog.catalog.generated_at.clone(),
    expires_at: catalog.catalog.expires_at.clone(),
    unsigned_community_build: catalog.catalog.unsigned_community_build,
    artifacts: runtime_update_artifacts(catalog, runtime_root)?,
  })
}

fn runtime_update_artifacts(
  catalog: &ValidatedRuntimeCatalog,
  runtime_root: &Path,
) -> Result<Vec<RuntimeUpdateArtifact>> {
  let (platform, architecture) = runtime_update_target()?;
  let installed_versions = list_installed_versions(runtime_root, "php")?;
  let artifacts = catalog
    .catalog
    .runtimes
    .iter()
    .filter(|release| release.platform == platform && release.architecture == architecture)
    .map(|release| {
      Ok(RuntimeUpdateArtifact {
        name: release.name.clone(),
        version: release.version.clone(),
        platform: release.platform.clone(),
        architecture: release.architecture.clone(),
        minimum_os_version: release
          .minimum_os_version
          .clone()
          .context("validated Runtime entry is missing minimumOsVersion")?,
        file_name: release
          .file_name
          .clone()
          .context("validated Runtime entry is missing fileName")?,
        size: release.size,
        sha256: release.sha256.clone(),
        unsigned_community_build: catalog.catalog.unsigned_community_build,
        installed: installed_versions.contains(&release.version),
      })
    })
    .collect::<Result<Vec<_>>>()?;
  if artifacts.is_empty() {
    bail!("Runtime Catalog has no package for {platform}/{architecture}");
  }
  Ok(artifacts)
}

fn runtime_update_target() -> Result<(&'static str, &'static str)> {
  let platform = if cfg!(target_os = "macos") {
    "macos"
  } else if cfg!(target_os = "windows") {
    "windows"
  } else {
    bail!("Runtime online updates are not supported on this platform");
  };
  let architecture = if cfg!(target_arch = "aarch64") {
    "arm64"
  } else if cfg!(target_arch = "x86_64") {
    "x64"
  } else {
    bail!("Runtime online updates are not supported on this architecture");
  };
  Ok((platform, architecture))
}

async fn stop_all(state: &AgentState) -> Result<()> {
  state.lan_share.lock().await.stop().await?;
  state.services.lock().await.stop_all().await
}

async fn stop_for_shutdown(state: &AgentState) -> Result<()> {
  state.lan_share.lock().await.stop().await?;
  state.proxy_manager.lock().await.stop_all().await;
  state.services.lock().await.shutdown().await
}

fn discover_lan_ipv4() -> Result<Ipv4Addr> {
  let socket =
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).context("unable to create LAN address probe")?;
  socket
    .connect((Ipv4Addr::new(192, 0, 2, 1), 9))
    .context("unable to select a LAN network route")?;
  let address = socket.local_addr()?.ip();
  match address {
    std::net::IpAddr::V4(address) if !address.is_loopback() && !address.is_unspecified() => {
      Ok(address)
    }
    _ => bail!("unable to discover a usable LAN IPv4 address"),
  }
}

async fn ensure_site_home(state: &AgentState) -> Result<PathBuf> {
  let configured = state.sites.lock().await.site_home()?;
  let path = configured
    .or_else(default_site_home)
    .context("unable to locate the user Home directory")?;
  if !path.is_absolute() {
    bail!("Site Home path must be absolute");
  }
  std::fs::create_dir_all(&path)
    .with_context(|| format!("unable to create Site Home directory: {}", path.display()))?;
  if !path.is_dir() {
    bail!("Site Home path is not a directory: {}", path.display());
  }
  let path = path
    .canonicalize()
    .with_context(|| format!("unable to resolve Site Home directory: {}", path.display()))?;
  state.sites.lock().await.save_site_home(&path)?;
  Ok(path)
}

async fn load_site_home_settings(state: &AgentState) -> Result<SiteHomeSettings> {
  let path = ensure_site_home(state).await?;
  sync_home_sites(state).await?;
  let home_sites = state.sites.lock().await.list_home_sites()?;
  let site_ids = home_sites.iter().map(|site| site.id).collect();
  let symbolic_link_site_ids = symbolic_link_site_ids(&path, &home_sites)?;
  Ok(SiteHomeSettings {
    path,
    site_ids,
    symbolic_link_site_ids,
  })
}

async fn save_site_home(state: &AgentState, path: &Path) -> Result<SiteHomeSettings> {
  if !path.is_absolute() {
    bail!("Site Home path must be absolute");
  }
  let previous = ensure_site_home(state).await?;
  std::fs::create_dir_all(path)
    .with_context(|| format!("unable to create Site Home directory: {}", path.display()))?;
  if !path.is_dir() {
    bail!("Site Home path is not a directory: {}", path.display());
  }
  let path = path
    .canonicalize()
    .with_context(|| format!("unable to resolve Site Home directory: {}", path.display()))?;
  state.sites.lock().await.save_site_home(&path)?;
  if let Err(error) = sync_home_sites(state).await {
    state.sites.lock().await.save_site_home(&previous)?;
    let _ = sync_home_sites(state).await;
    return Err(error.context("unable to apply Site Home directory"));
  }
  let home_sites = state.sites.lock().await.list_home_sites()?;
  let site_ids = home_sites.iter().map(|site| site.id).collect();
  let symbolic_link_site_ids = symbolic_link_site_ids(&path, &home_sites)?;
  Ok(SiteHomeSettings {
    path,
    site_ids,
    symbolic_link_site_ids,
  })
}

async fn sync_home_sites(state: &AgentState) -> Result<Vec<Site>> {
  let home = ensure_site_home(state).await?;
  let default_php = active_version(&state.paths.runtimes, "php")?
    .map(|version| php_series(&version))
    .transpose()?
    .map(|series| series.parse())
    .transpose()
    .context("invalid global PHP Runtime series")?;

  let (existing_home, linked_sites) = {
    let repository = state.sites.lock().await;
    let existing_home = repository.list_home_sites()?;
    let home_ids = existing_home
      .iter()
      .map(|site| site.id)
      .collect::<HashSet<_>>();
    let linked_sites = repository
      .list()?
      .into_iter()
      .filter(|site| !home_ids.contains(&site.id))
      .collect::<Vec<_>>();
    (existing_home, linked_sites)
  };
  let desired_home = discover_home_sites(&home, default_php, &linked_sites, &existing_home)?;
  let desired_ids = desired_home
    .iter()
    .map(|site| site.id)
    .collect::<HashSet<_>>();
  let existing_by_id = existing_home
    .iter()
    .map(|site| (site.id, site))
    .collect::<HashMap<_, _>>();
  let removed = existing_home
    .iter()
    .filter(|site| !desired_ids.contains(&site.id))
    .cloned()
    .collect::<Vec<_>>();
  let added_or_changed = desired_home
    .iter()
    .filter(|site| {
      existing_by_id
        .get(&site.id)
        .is_none_or(|existing| *existing != *site)
    })
    .cloned()
    .collect::<Vec<_>>();

  let sites = {
    let mut repository = state.sites.lock().await;
    repository.replace_home_sites(&desired_home)?;
    repository.list()?
  };
  if removed.is_empty() && added_or_changed.is_empty() {
    return Ok(sites);
  }

  let apply_result = {
    let mut services = state.services.lock().await;
    async {
      services.sync_site_domains(&sites).await?;
      for site in &removed {
        services.remove_site_config(site, &sites).await?;
      }
      for site in &added_or_changed {
        services.add_site_config(site).await?;
      }
      Ok::<(), anyhow::Error>(())
    }
    .await
  };
  let Err(error) = apply_result else {
    return Ok(sites);
  };

  let restored_sites = {
    let mut repository = state.sites.lock().await;
    repository
      .replace_home_sites(&existing_home)
      .context("unable to restore Site Home registry after service sync failed")?;
    repository.list()?
  };
  let existing_domains = existing_home
    .iter()
    .map(|site| site.domain.as_str())
    .collect::<HashSet<_>>();
  let rollback_result = {
    let mut services = state.services.lock().await;
    async {
      services.sync_site_domains(&restored_sites).await?;
      for site in desired_home
        .iter()
        .filter(|site| !existing_domains.contains(site.domain.as_str()))
      {
        services.remove_site_config(site, &restored_sites).await?;
      }
      for site in &existing_home {
        services.add_site_config(site).await?;
      }
      Ok::<(), anyhow::Error>(())
    }
    .await
  };
  match rollback_result {
    Ok(()) => Err(error),
    Err(rollback_error) => Err(error.context(format!(
      "Site Home service rollback also failed: {rollback_error}"
    ))),
  }
}

fn discover_home_sites(
  home: &Path,
  default_php: Option<fabdev_core::PhpVersion>,
  linked_sites: &[Site],
  existing_home: &[Site],
) -> Result<Vec<Site>> {
  let resolved_home = home
    .canonicalize()
    .with_context(|| format!("unable to resolve Site Home directory: {}", home.display()))?;
  let existing_by_path = existing_home
    .iter()
    .map(|site| (site.project_path.clone(), site))
    .collect::<HashMap<_, _>>();
  let mut reserved_domains = linked_sites
    .iter()
    .map(|site| site.domain.clone())
    .collect::<HashSet<_>>();
  let mut directories = std::fs::read_dir(home)
    .with_context(|| format!("unable to read Site Home directory: {}", home.display()))?
    .collect::<std::io::Result<Vec<_>>>()?;
  directories.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

  let mut sites = Vec::new();
  let mut discovered_paths = HashSet::new();
  for entry in directories {
    let name = entry.file_name();
    if name.to_string_lossy().starts_with('.') {
      continue;
    }
    let entry_path = entry.path();
    let entry_type = entry.file_type()?;
    let (project_path, site_name, domain) = if entry_type.is_dir() {
      (entry_path.clone(), None, None)
    } else if entry_type.is_symlink() {
      let Ok(target) = entry_path.canonicalize() else {
        continue;
      };
      if target == resolved_home || !target.is_dir() {
        continue;
      }
      let site_name = name.to_string_lossy().into_owned();
      let domain = default_site_domain(&site_name);
      (target, Some(site_name), Some(domain))
    } else {
      continue;
    };
    let mut site = create_site(SiteInput {
      name: site_name,
      domain,
      project_path,
      document_root: None,
      php_version: default_php.clone(),
    })
    .with_context(|| format!("unable to create Home Site from {}", entry_path.display()))?;
    if reserved_domains.contains(&site.domain) {
      continue;
    }
    if !discovered_paths.insert(site.project_path.clone()) {
      continue;
    }
    if let Some(existing) = existing_by_path.get(&site.project_path) {
      site.id = existing.id;
      site.php_version = existing.php_version.clone();
      site.enabled = existing.enabled;
      site.secured = existing.secured;
    }
    reserved_domains.insert(site.domain.clone());
    sites.push(site);
  }
  Ok(sites)
}

fn symbolic_link_site_ids(home: &Path, home_sites: &[Site]) -> Result<Vec<uuid::Uuid>> {
  let mut symbolic_links = HashSet::new();
  for entry in std::fs::read_dir(home)
    .with_context(|| format!("unable to read Site Home directory: {}", home.display()))?
  {
    let entry = entry?;
    let name = entry.file_name();
    if name.to_string_lossy().starts_with('.') || !entry.file_type()?.is_symlink() {
      continue;
    }
    let Ok(target) = entry.path().canonicalize() else {
      continue;
    };
    if !target.is_dir() {
      continue;
    }
    symbolic_links.insert((target, default_site_domain(&name.to_string_lossy())));
  }

  Ok(
    home_sites
      .iter()
      .filter(|site| symbolic_links.contains(&(site.project_path.clone(), site.domain.clone())))
      .map(|site| site.id)
      .collect(),
  )
}

async fn php_runtime_state(state: &AgentState) -> Result<PhpRuntimeState> {
  let sites = state.sites.lock().await.list()?;
  let runtime_root = state.paths.runtimes.clone();
  tokio::task::spawn_blocking(move || build_php_runtime_state(&runtime_root, &sites))
    .await
    .context("Runtime state task failed")?
}

async fn node_runtime_state(state: &AgentState) -> Result<NodeRuntimeState> {
  let runtime_root = state.paths.runtimes.clone();
  tokio::task::spawn_blocking(move || build_node_runtime_state(&runtime_root))
    .await
    .context("Node.js Runtime state task failed")?
}

fn build_node_runtime_state(runtime_root: &Path) -> Result<NodeRuntimeState> {
  let installed_version = active_version(runtime_root, "node")?.filter(|version| {
    version == STABLE_NODE_VERSION && node_runtime_binary(runtime_root, version).is_file()
  });
  Ok(NodeRuntimeState {
    stable_version: STABLE_NODE_VERSION.to_owned(),
    installed_version,
  })
}

fn node_runtime_binary(runtime_root: &Path, version: &str) -> PathBuf {
  let runtime = runtime_root.join("node").join(version);
  if cfg!(windows) {
    runtime.join("node.exe")
  } else {
    runtime.join("bin/node")
  }
}

fn build_php_runtime_state(
  runtime_root: &std::path::Path,
  sites: &[Site],
) -> Result<PhpRuntimeState> {
  let global_version = active_version(runtime_root, "php")?;
  let installed = list_installed_versions(runtime_root, "php")?
    .into_iter()
    .filter(|version| php_runtime_binary(runtime_root, version).is_file())
    .map(|version| {
      let series = php_series(&version)?;
      let runtime_sites = sites
        .iter()
        .filter(|site| {
          site
            .php_version
            .as_ref()
            .is_some_and(|version| version.to_string() == series)
        })
        .map(|site| site.domain.clone())
        .collect();
      Ok(PhpRuntimeInfo {
        active: global_version.as_deref() == Some(&version),
        version,
        series,
        sites: runtime_sites,
      })
    })
    .collect::<Result<Vec<_>>>()?;
  Ok(PhpRuntimeState {
    global_version,
    installed,
  })
}

fn php_runtime_binary(runtime_root: &std::path::Path, version: &str) -> PathBuf {
  let runtime = runtime_root.join("php").join(version);
  if cfg!(windows) {
    runtime.join("php-cgi.exe")
  } else {
    runtime.join("sbin/php-fpm")
  }
}

fn active_mariadb_runtime_path(runtime_root: &Path) -> Result<PathBuf> {
  Ok(match active_version(runtime_root, "mariadb")? {
    Some(version) => runtime_root.join("mariadb").join(version),
    None => runtime_root.join("mariadb/current"),
  })
}

fn web_services_ready(status: &AgentStatus, requires_php: bool) -> bool {
  status.dns == ServiceState::Running
    && status.nginx == ServiceState::Running
    && (!requires_php || status.php_fpm == ServiceState::Running)
}

fn web_services_need_cleanup(status: &AgentStatus) -> bool {
  [&status.dns, &status.nginx, &status.php_fpm]
    .into_iter()
    .any(|state| {
      matches!(
        state,
        ServiceState::Starting
          | ServiceState::Running
          | ServiceState::Stopping
          | ServiceState::Failed
      )
    })
}

fn php_series(version: &str) -> Result<String> {
  let parts = version.split('.').collect::<Vec<_>>();
  if parts.len() != 3 || parts.iter().any(|part| part.parse::<u16>().is_err()) {
    bail!("invalid PHP Runtime version: {version}");
  }
  Ok(format!("{}.{}", parts[0], parts[1]))
}

fn validate_php_release(release: &RuntimeRelease, artifact: &std::path::Path) -> Result<()> {
  if release.name != "php" {
    bail!("Runtime package must contain PHP, got {}", release.name);
  }
  let series = php_series(&release.version)?;
  if !matches!(series.as_str(), "7.4" | "8.2" | "8.3" | "8.4") {
    bail!("unsupported PHP Runtime series: {series}");
  }
  validate_release_target(release, artifact)
}

fn validate_mariadb_release(release: &RuntimeRelease, artifact: &std::path::Path) -> Result<()> {
  if release.name != "mariadb" {
    bail!("Runtime package must contain MariaDB, got {}", release.name);
  }
  if release.version != "12.3.2" {
    bail!("unsupported MariaDB Runtime version: {}", release.version);
  }
  validate_release_target(release, artifact)
}

fn validate_node_release(release: &RuntimeRelease, artifact: &Path) -> Result<()> {
  if release.name != "node" {
    bail!("Runtime package must contain Node.js, got {}", release.name);
  }
  if release.version != STABLE_NODE_VERSION {
    bail!("unsupported Node.js Runtime version: {}", release.version);
  }
  validate_release_target(release, artifact)
}

fn validate_release_target(release: &RuntimeRelease, artifact: &std::path::Path) -> Result<()> {
  let platform = if cfg!(target_os = "macos") {
    "macos"
  } else if cfg!(target_os = "windows") {
    "windows"
  } else {
    "unsupported"
  };
  let architecture = if cfg!(target_arch = "aarch64") {
    "arm64"
  } else if cfg!(target_arch = "x86_64") {
    "x64"
  } else {
    "unsupported"
  };
  if release.platform != platform || release.architecture != architecture {
    bail!(
      "Runtime package targets {} {}, expected {platform} {architecture}",
      release.platform,
      release.architecture
    );
  }
  let size = std::fs::metadata(artifact)
    .with_context(|| format!("unable to read Runtime artifact: {}", artifact.display()))?
    .len();
  if size != release.size {
    bail!(
      "Runtime artifact size mismatch: expected {}, got {size}",
      release.size
    );
  }
  Ok(())
}

fn internal_error(error: impl std::fmt::Display) -> AgentResponse {
  AgentResponse::Error {
    code: "internal_error".to_owned(),
    message: error.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use fabdev_core::PhpVersion;
  use uuid::Uuid;

  use super::*;

  #[test]
  fn tracks_runtime_download_progress_and_cancellation() {
    let operation_id = Uuid::new_v4();
    let task = RuntimeDownloadTask::new(RuntimeUpdateOperation {
      operation_id,
      status: RuntimeUpdateOperationStatus::Queued,
      name: "php".to_owned(),
      version: "8.4.24".to_owned(),
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      file_name: "php-8.4.24-macos-arm64-community.tar.gz".to_owned(),
      bytes_downloaded: 0,
      total_bytes: 100,
      sha256: "a".repeat(64),
      error: None,
    });

    assert!(task.begin_download());
    task.set_progress(25);
    assert_eq!(task.snapshot().bytes_downloaded, 25);
    assert_eq!(
      task.cancel().expect("cancel Runtime download").status,
      RuntimeUpdateOperationStatus::Cancelled
    );
    assert!(task.is_cancelled());
    assert!(task.cancel().is_err());
  }

  #[tokio::test]
  async fn removes_one_shared_site_without_stopping_the_others() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut state = LanShareState::new(8080);
    state.info = Some(LanShareInfo {
      host: "192.168.1.10".to_owned(),
      port: 18080,
      sites: vec![
        LanShareSiteInfo {
          site_id: first,
          domain: "site-one.test".to_owned(),
        },
        LanShareSiteInfo {
          site_id: second,
          domain: "site-two.test".to_owned(),
        },
      ],
    });

    assert!(state.contains(first));
    assert!(state.contains(second));
    let remaining = state
      .stop_site(first)
      .await
      .expect("stop one shared Site")
      .expect("keep remaining share");
    assert_eq!(remaining.sites.len(), 1);
    assert_eq!(remaining.sites[0].site_id, second);
    assert!(!state.contains(first));
    assert!(state.contains(second));
    assert!(state
      .stop_site(second)
      .await
      .expect("stop final shared Site")
      .is_none());
    assert!(state.info().is_none());
  }

  #[tokio::test]
  async fn updates_the_domain_of_an_existing_shared_site() {
    let site_id = Uuid::new_v4();
    let mut state = LanShareState::new(8080);
    state.info = Some(LanShareInfo {
      host: "192.168.1.10".to_owned(),
      port: 18080,
      sites: vec![LanShareSiteInfo {
        site_id,
        domain: "old.test".to_owned(),
      }],
    });
    let site = Site {
      id: site_id,
      name: "ERP".to_owned(),
      domain: "new.test".to_owned(),
      project_path: "/tmp/erp".into(),
      document_root: "/tmp/erp/public".into(),
      php_version: None,
      enabled: true,
      secured: false,
    };

    state.update_site(&site).await.expect("update shared Site");

    let info = state.info().expect("keep LAN share");
    assert_eq!(info.sites[0].site_id, site_id);
    assert_eq!(info.sites[0].domain, "new.test");
  }

  #[test]
  fn reports_installed_global_and_site_runtime_state() {
    let root = std::env::temp_dir().join(format!("fabdev-agent-runtime-{}", Uuid::new_v4()));
    for version in ["7.4.33", "8.2.33"] {
      let binary = php_runtime_binary(&root, version);
      std::fs::create_dir_all(binary.parent().expect("PHP binary parent"))
        .expect("create PHP fixture");
      std::fs::write(binary, "fixture").expect("write PHP fixture");
    }
    set_active_version(&root, "php", "8.2.33").expect("set active PHP");
    let sites = vec![Site {
      id: Uuid::new_v4(),
      name: "ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/erp".into(),
      document_root: "/tmp/erp/public".into(),
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    }];

    let state = build_php_runtime_state(&root, &sites).expect("build Runtime state");
    assert_eq!(state.global_version.as_deref(), Some("8.2.33"));
    assert_eq!(state.installed.len(), 2);
    assert_eq!(state.installed[0].sites, vec!["erp.test"]);
    assert!(state.installed[0].active);
    assert!(state.installed[1].sites.is_empty());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn reports_optional_node_runtime_state() {
    let root = std::env::temp_dir().join(format!("fabdev-agent-node-{}", Uuid::new_v4()));
    let empty = build_node_runtime_state(&root).expect("build empty Node.js state");
    assert_eq!(empty.stable_version, STABLE_NODE_VERSION);
    assert_eq!(empty.installed_version, None);

    let binary = node_runtime_binary(&root, STABLE_NODE_VERSION);
    std::fs::create_dir_all(binary.parent().expect("Node.js binary parent"))
      .expect("create Node.js fixture");
    std::fs::write(binary, "fixture").expect("write Node.js fixture");
    set_active_version(&root, "node", STABLE_NODE_VERSION).expect("activate Node.js fixture");
    let installed = build_node_runtime_state(&root).expect("build Node.js state");
    assert_eq!(
      installed.installed_version.as_deref(),
      Some(STABLE_NODE_VERSION)
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rejects_non_php_runtime_release() {
    let root = std::env::temp_dir().join(format!("fabdev-agent-release-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, "fixture").expect("write artifact");
    let release = RuntimeRelease {
      name: "nginx".to_owned(),
      version: "1.30.4".to_owned(),
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      url: "runtime.tar.gz".to_owned(),
      size: 7,
      sha256: "fixture".to_owned(),
      signature: Some("development-ad-hoc".to_owned()),
      ..RuntimeRelease::default()
    };

    let error = validate_php_release(&release, &artifact).expect_err("reject Nginx package");
    assert!(error.to_string().contains("must contain PHP"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn validates_supported_php_84_runtime_release() {
    let root = std::env::temp_dir().join(format!("fabdev-php84-release-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, "fixture").expect("write artifact");
    let release = RuntimeRelease {
      name: "php".to_owned(),
      version: "8.4.24".to_owned(),
      platform: if cfg!(target_os = "macos") {
        "macos".to_owned()
      } else {
        "windows".to_owned()
      },
      architecture: if cfg!(target_arch = "aarch64") {
        "arm64".to_owned()
      } else {
        "x64".to_owned()
      },
      url: "runtime.tar.gz".to_owned(),
      size: 7,
      sha256: "fixture".to_owned(),
      signature: Some("development-ad-hoc".to_owned()),
      ..RuntimeRelease::default()
    };

    validate_php_release(&release, &artifact).expect("accept PHP 8.4 package");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn validates_only_the_pinned_node_runtime_release() {
    let root = std::env::temp_dir().join(format!("fabdev-node-release-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, "fixture").expect("write artifact");
    let mut release = RuntimeRelease {
      name: "node".to_owned(),
      version: STABLE_NODE_VERSION.to_owned(),
      platform: if cfg!(target_os = "macos") {
        "macos".to_owned()
      } else {
        "windows".to_owned()
      },
      architecture: if cfg!(target_arch = "aarch64") {
        "arm64".to_owned()
      } else {
        "x64".to_owned()
      },
      url: "runtime.tar.gz".to_owned(),
      size: 7,
      sha256: "fixture".to_owned(),
      signature: Some("development-ad-hoc".to_owned()),
      ..RuntimeRelease::default()
    };

    validate_node_release(&release, &artifact).expect("accept pinned Node.js package");
    release.version = "24.18.0".to_owned();
    let error = validate_node_release(&release, &artifact).expect_err("reject stale Node.js");
    assert!(error.to_string().contains("unsupported Node.js Runtime"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn discovers_first_level_home_sites_and_ignores_hidden_and_nested_folders() {
    let home = std::env::temp_dir().join(format!("fabdev-site-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(home.join("site1/public")).expect("create Site Home project");
    std::fs::write(home.join("site1/public/index.php"), "<?php echo 'site1';")
      .expect("write Site Home fixture");
    std::fs::create_dir_all(home.join("group/site2")).expect("create nested project");
    std::fs::create_dir_all(home.join(".hidden")).expect("create hidden project");

    let sites = discover_home_sites(&home, Some("8.2".parse().expect("parse PHP")), &[], &[])
      .expect("discover Home Sites");

    assert_eq!(sites.len(), 2);
    assert_eq!(sites[0].domain, "group.test");
    assert_eq!(
      sites[0].document_root,
      home.join("group").canonicalize().expect("resolve group")
    );
    assert_eq!(sites[1].domain, "site1.test");
    assert_eq!(
      sites[1].document_root,
      home
        .join("site1/public")
        .canonicalize()
        .expect("resolve public")
    );
    assert!(sites.iter().all(|site| site.domain != "site2.test"));
    assert!(symbolic_link_site_ids(&home, &sites)
      .expect("derive symbolic link Site IDs")
      .is_empty());
    std::fs::remove_dir_all(home).expect("remove Site Home fixture");
  }

  #[cfg(unix)]
  #[test]
  fn discovers_first_level_directory_symlinks_and_ignores_invalid_targets() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("fabdev-site-home-link-{}", Uuid::new_v4()));
    let home = root.join("home");
    let target = root.join("projects/site-one/app");
    std::fs::create_dir_all(target.join("public")).expect("create linked Site project");
    std::fs::write(target.join("public/index.php"), "<?php echo 'site-one';")
      .expect("write linked Site fixture");
    std::fs::create_dir_all(&home).expect("create Site Home fixture");
    symlink(&target, home.join("site-one")).expect("create Site Home directory symlink");
    symlink(&target, home.join("site-one-copy")).expect("create duplicate Site Home symlink");
    symlink(root.join("missing"), home.join("broken")).expect("create broken Site Home symlink");
    symlink(&home, home.join("self")).expect("create self-referencing Site Home symlink");
    symlink(home.join("cycle-b"), home.join("cycle-a")).expect("create first cyclic symlink");
    symlink(home.join("cycle-a"), home.join("cycle-b")).expect("create second cyclic symlink");

    let sites = discover_home_sites(&home, Some("8.2".parse().expect("parse PHP")), &[], &[])
      .expect("discover symlinked Home Sites");

    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].name, "site-one");
    assert_eq!(sites[0].domain, "site-one.test");
    assert_eq!(
      sites[0].project_path,
      target.canonicalize().expect("resolve linked Site project")
    );
    assert_eq!(
      sites[0].document_root,
      target
        .join("public")
        .canonicalize()
        .expect("resolve linked Site document root")
    );
    assert_eq!(
      symbolic_link_site_ids(&home, &sites).expect("derive symbolic link Site IDs"),
      vec![sites[0].id]
    );
    std::fs::remove_dir_all(root).expect("remove Site Home symlink fixture");
  }

  #[test]
  fn preserves_home_site_settings_and_gives_linked_sites_domain_priority() {
    let home = std::env::temp_dir().join(format!("fabdev-site-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(home.join("erp")).expect("create ERP project");
    std::fs::create_dir_all(home.join("site1")).expect("create Site project");
    let existing = create_site(SiteInput {
      name: None,
      domain: None,
      project_path: home.join("site1"),
      document_root: None,
      php_version: Some("7.4".parse().expect("parse PHP")),
    })
    .expect("create existing Home Site");
    let linked = Site {
      id: Uuid::new_v4(),
      name: "Linked ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/linked-erp".into(),
      document_root: "/tmp/linked-erp/public".into(),
      php_version: Some("8.2".parse().expect("parse PHP")),
      enabled: true,
      secured: false,
    };

    let sites = discover_home_sites(
      &home,
      Some("8.2".parse().expect("parse PHP")),
      &[linked],
      std::slice::from_ref(&existing),
    )
    .expect("discover Home Sites");

    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].id, existing.id);
    assert_eq!(sites[0].php_version, existing.php_version);
    assert_eq!(sites[0].domain, "site1.test");
    std::fs::remove_dir_all(home).expect("remove Site Home fixture");
  }

  #[test]
  fn makes_web_service_start_idempotent_and_repairs_partial_state() {
    let mut status = AgentStatus::development();
    status.dns = ServiceState::Running;
    status.nginx = ServiceState::Running;
    status.php_fpm = ServiceState::Running;
    assert!(web_services_ready(&status, true));
    assert!(web_services_ready(&status, false));

    status.php_fpm = ServiceState::Installed;
    assert!(!web_services_ready(&status, true));
    assert!(web_services_ready(&status, false));

    status.nginx = ServiceState::Failed;
    assert!(!web_services_ready(&status, false));
    assert!(web_services_need_cleanup(&status));

    status.dns = ServiceState::Installed;
    status.nginx = ServiceState::Installed;
    assert!(!web_services_need_cleanup(&status));
  }

  #[test]
  fn validates_supported_mariadb_runtime_release() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-release-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, "fixture").expect("write artifact");
    let release = RuntimeRelease {
      name: "mariadb".to_owned(),
      version: "12.3.2".to_owned(),
      platform: if cfg!(target_os = "macos") {
        "macos".to_owned()
      } else {
        "windows".to_owned()
      },
      architecture: if cfg!(target_arch = "aarch64") {
        "arm64".to_owned()
      } else {
        "x64".to_owned()
      },
      url: "runtime.tar.gz".to_owned(),
      size: 7,
      sha256: "fixture".to_owned(),
      signature: Some("development-ad-hoc".to_owned()),
      ..RuntimeRelease::default()
    };

    validate_mariadb_release(&release, &artifact).expect("accept MariaDB package");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn resolves_the_active_mariadb_runtime_version() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-active-{}", Uuid::new_v4()));
    let runtime = root.join("mariadb/12.3.2");
    std::fs::create_dir_all(&runtime).expect("create MariaDB Runtime fixture");
    set_active_version(&root, "mariadb", "12.3.2").expect("activate MariaDB fixture");

    assert_eq!(
      active_mariadb_runtime_path(&root).expect("resolve active MariaDB Runtime"),
      runtime
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }
}
