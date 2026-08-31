use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{bail, Context, Result};
use clap::Parser;
use fabdev_core::{
  create_site, default_site_domain, default_site_home, edit_site, AgentEndpoint, AgentRequest,
  AgentResponse, AgentStatus, AppPaths, LanShareInfo, LanShareSiteInfo, NodeRuntimeInfo,
  NodeRuntimeState, PhpRuntimeInfo, PhpRuntimeState, PhpVersion, RuntimeUpdateArtifact,
  RuntimeUpdateCheck, RuntimeUpdateOperation, RuntimeUpdateOperationStatus, ServiceState, Site,
  SiteHomeSettings, SiteInput, SiteRepository, TerminalNodeState, TerminalPhpState,
  PROTOCOL_VERSION, SUPPORTED_NODE_VERSIONS,
};
use fabdev_platform::{
  disable_terminal_node, disable_terminal_php, enable_terminal_node, enable_terminal_php,
  terminal_node_state, terminal_php_state,
};
use fabdev_proxy::ProxyManager;
use fabdev_runtime::{
  active_version, deactivate_runtime, install_tar_gz_with_activation,
  install_tar_gz_with_health_check, list_installed_versions, mark_runtime_removed,
  remove_installed_version, set_active_version, RuntimeError, RuntimeRelease,
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

  fn begin_install(&self) -> Result<RuntimeUpdateOperation> {
    let mut snapshot = self
      .snapshot
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if snapshot.status != RuntimeUpdateOperationStatus::Verified {
      bail!("Runtime package must be verified before installation");
    }
    snapshot.status = RuntimeUpdateOperationStatus::Installing;
    snapshot.error = None;
    drop(snapshot);
    Ok(self.snapshot())
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

  async fn begin_install(&self, operation_id: uuid::Uuid) -> Result<RuntimeUpdateOperation> {
    let operation = self
      .operations
      .lock()
      .await
      .get(&operation_id)
      .cloned()
      .context("Runtime update operation was not found")?;
    operation.begin_install()
  }

  async fn finish_install(
    &self,
    operation_id: uuid::Uuid,
    error: Option<String>,
  ) -> Result<RuntimeUpdateOperation> {
    let operation = self
      .operations
      .lock()
      .await
      .get(&operation_id)
      .cloned()
      .context("Runtime update operation was not found")?;
    let status = if error.is_some() {
      RuntimeUpdateOperationStatus::Failed
    } else {
      RuntimeUpdateOperationStatus::Completed
    };
    operation.set_status(status, error);
    Ok(operation.snapshot())
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
      match install_downloaded_runtime(state, operation_id).await {
        Ok(operation) => AgentResponse::RuntimeUpdateOperation(operation),
        Err(error) => AgentResponse::Error {
          code: "runtime_install_failed".to_owned(),
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
    AgentRequest::GetTerminalPhp => terminal_php_response(&state.paths, TerminalPhpAction::Get),
    AgentRequest::EnableTerminalPhp => {
      terminal_php_response(&state.paths, TerminalPhpAction::Enable)
    }
    AgentRequest::DisableTerminalPhp => {
      terminal_php_response(&state.paths, TerminalPhpAction::Disable)
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
          false,
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
    AgentRequest::SetGlobalNode { version } => {
      let runtime_root = state.paths.runtimes.clone();
      let data_root = state.paths.root.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        if !node_runtime_binary(&runtime_root, &version).is_file() {
          bail!("Node.js Runtime {version} is not installed");
        }
        let active_before = active_version(&runtime_root, "node")?;
        set_active_version(&runtime_root, "node", &version)?;
        if let Err(error) = enable_terminal_node(&data_root) {
          restore_active_runtime(&runtime_root, "node", active_before.as_deref())?;
          return Err(error.into());
        }
        Ok(())
      })
      .await;
      match result {
        Ok(Ok(())) => match node_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::GlobalNodeChanged(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "global_node_change_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::EnableTerminalNode => {
      let runtime_root = state.paths.runtimes.clone();
      let data_root = state.paths.root.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        let version = active_version(&runtime_root, "node")?
          .context("select a global Node.js version before enabling terminal integration")?;
        if !node_runtime_binary(&runtime_root, &version).is_file() {
          bail!("global Node.js Runtime {version} is missing node");
        }
        enable_terminal_node(data_root)?;
        Ok(())
      })
      .await;
      match result {
        Ok(Ok(())) => match node_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::TerminalNode(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "terminal_node_integration_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::DisableTerminalNode => {
      let data_root = state.paths.root.clone();
      let result = tokio::task::spawn_blocking(move || disable_terminal_node(data_root)).await;
      match result {
        Ok(Ok(_)) => match node_runtime_state(state).await {
          Ok(runtime_state) => AgentResponse::TerminalNode(runtime_state),
          Err(error) => internal_error(error),
        },
        Ok(Err(error)) => AgentResponse::Error {
          code: "terminal_node_integration_failed".to_owned(),
          message: error.to_string(),
        },
        Err(error) => internal_error(error),
      }
    }
    AgentRequest::RemoveNodeRuntime { version } => {
      let runtime_root = state.paths.runtimes.clone();
      let data_root = state.paths.root.clone();
      let result = tokio::task::spawn_blocking(move || -> Result<()> {
        if !node_runtime_binary(&runtime_root, &version).is_file() {
          bail!("installed Node.js Runtime is missing node: {version}");
        }
        let was_active = active_version(&runtime_root, "node")?.as_deref() == Some(&version);
        if was_active {
          deactivate_runtime(&runtime_root, "node")?;
          if let Err(error) = disable_terminal_node(&data_root) {
            set_active_version(&runtime_root, "node", &version)?;
            return Err(error.into());
          }
        }
        if let Err(error) = remove_installed_version(&runtime_root, "node", &version) {
          if was_active {
            set_active_version(&runtime_root, "node", &version).with_context(|| {
              format!("unable to restore Node.js Runtime {version} after removal failed")
            })?;
            enable_terminal_node(&data_root)?;
          }
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

async fn install_downloaded_runtime(
  state: &AgentState,
  operation_id: uuid::Uuid,
) -> Result<RuntimeUpdateOperation> {
  let operation = state.runtime_updates.begin_install(operation_id).await?;
  let result = install_downloaded_runtime_inner(state, &operation).await;
  match result {
    Ok(()) => {
      state
        .runtime_updates
        .finish_install(operation_id, None)
        .await
    }
    Err(error) => {
      state
        .runtime_updates
        .finish_install(operation_id, Some(format!("{error:#}")))
        .await
    }
  }
}

async fn install_downloaded_runtime_inner(
  state: &AgentState,
  operation: &RuntimeUpdateOperation,
) -> Result<()> {
  if !online_runtime_supported(&operation.name, &operation.platform, &operation.version) {
    bail!(
      "online Runtime installation does not support {} {} for {}",
      operation.name,
      operation.version,
      operation.platform
    );
  }
  let downloaded =
    fabdev_updater::verified_cached_runtime_update(fabdev_updater::RuntimeDownloadRequest {
      cache_directory: &state.paths.cache,
      current_app_version: env!("CARGO_PKG_VERSION"),
      current_agent_protocol_version: PROTOCOL_VERSION,
      name: &operation.name,
      version: &operation.version,
      platform: &operation.platform,
      architecture: &operation.architecture,
    })
    .await
    .context("Runtime package revalidation failed before installation")?;

  match downloaded.name.as_str() {
    "php" => install_downloaded_php_runtime(state, &downloaded).await,
    "node" => install_downloaded_node_runtime(state, &downloaded).await,
    "mariadb" => install_downloaded_mariadb_runtime(state, &downloaded).await,
    _ => bail!("unsupported downloaded Runtime: {}", downloaded.name),
  }
}

async fn install_downloaded_php_runtime(
  state: &AgentState,
  downloaded: &fabdev_updater::DownloadedRuntimeUpdate,
) -> Result<()> {
  let runtime_root = state.paths.runtimes.clone();
  let active_before = active_version(&runtime_root, "php")?;
  let runtime_destination = runtime_root.join("php").join(&downloaded.version);
  if runtime_destination.exists() {
    bail!(
      "Runtime {} {} is already installed",
      downloaded.name,
      downloaded.version
    );
  }
  let artifact_path = downloaded.path.clone();
  let expected_sha256 = downloaded.sha256.clone();
  let name = downloaded.name.clone();
  let version = downloaded.version.clone();
  let install_result = tokio::task::spawn_blocking(move || {
    install_tar_gz_with_health_check(
      artifact_path,
      &expected_sha256,
      &name,
      &version,
      runtime_root,
      false,
      |staged| validate_staged_php_runtime(staged, &version),
    )
  })
  .await
  .context("Runtime installation task failed")?;
  if let Err(error) = install_result {
    if runtime_destination.exists() {
      remove_online_runtime_directory(&runtime_destination)?;
    }
    return Err(error.into());
  }

  let php_version = php_series(&downloaded.version)?
    .parse::<PhpVersion>()
    .context("invalid installed PHP Runtime series")?;
  let config_directory = state.paths.config.join("php").join(php_version.to_string());
  let service_directory = state
    .paths
    .services
    .join("php")
    .join(php_version.to_string());
  let config_existed = config_directory.exists();
  let service_existed = service_directory.exists();
  let validation = state
    .services
    .lock()
    .await
    .validate_php_runtime_install(&php_version, &downloaded.version);
  if let Err(error) = validation {
    rollback_online_php_install(
      &state.paths.runtimes,
      &downloaded.version,
      active_before.as_deref(),
      &config_directory,
      config_existed,
      &service_directory,
      service_existed,
    )?;
    return Err(error.context("installed PHP Runtime failed its fixed health check"));
  }
  let active_after = match active_version(&state.paths.runtimes, "php") {
    Ok(active_after) => active_after,
    Err(error) => {
      rollback_online_php_install(
        &state.paths.runtimes,
        &downloaded.version,
        active_before.as_deref(),
        &config_directory,
        config_existed,
        &service_directory,
        service_existed,
      )?;
      return Err(error.into());
    }
  };
  if active_after != active_before {
    rollback_online_php_install(
      &state.paths.runtimes,
      &downloaded.version,
      active_before.as_deref(),
      &config_directory,
      config_existed,
      &service_directory,
      service_existed,
    )?;
    bail!("online Runtime installation changed the active PHP version");
  }
  Ok(())
}

async fn install_downloaded_node_runtime(
  state: &AgentState,
  downloaded: &fabdev_updater::DownloadedRuntimeUpdate,
) -> Result<()> {
  let runtime_root = state.paths.runtimes.clone();
  let active_before = active_version(&runtime_root, "node")?;
  let runtime_destination = runtime_root.join("node").join(&downloaded.version);
  if runtime_destination.exists() {
    bail!("Runtime node {} is already installed", downloaded.version);
  }
  let artifact_path = downloaded.path.clone();
  let expected_sha256 = downloaded.sha256.clone();
  let version = downloaded.version.clone();
  let install_runtime_root = runtime_root.clone();
  let install_version = version.clone();
  let install_result = tokio::task::spawn_blocking(move || {
    install_tar_gz_with_health_check(
      artifact_path,
      &expected_sha256,
      "node",
      &install_version,
      install_runtime_root,
      false,
      |staged| validate_staged_node_runtime(staged, &install_version),
    )
  })
  .await
  .context("Node.js Runtime installation task failed")?;
  if let Err(error) = install_result {
    rollback_online_runtime_install(&runtime_root, "node", &version, active_before.as_deref())?;
    return Err(error.into());
  }
  if active_version(&runtime_root, "node")? != active_before {
    rollback_online_runtime_install(&runtime_root, "node", &version, active_before.as_deref())?;
    bail!("online Node.js Runtime installation changed the selected global version");
  }
  Ok(())
}

async fn install_downloaded_mariadb_runtime(
  state: &AgentState,
  downloaded: &fabdev_updater::DownloadedRuntimeUpdate,
) -> Result<()> {
  let active_before = active_version(&state.paths.runtimes, "mariadb")?;
  let was_running = matches!(
    state.services.lock().await.status().mariadb,
    fabdev_core::ServiceState::Running
  );
  if was_running {
    let stop_result = {
      let mut services = state.services.lock().await;
      match services.stop_mariadb().await {
        Ok(()) => services.refresh_php_mariadb_connection().await,
        Err(error) => Err(error),
      }
    };
    if let Err(error) = stop_result {
      let _ = restart_active_mariadb_runtime(state).await;
      return Err(error.context("unable to stop MariaDB safely before Runtime update"));
    }
  }

  let install_result = install_downloaded_mariadb_runtime_while_stopped(state, downloaded).await;
  if let Err(error) = install_result {
    if was_running {
      restart_active_mariadb_runtime(state)
        .await
        .context("unable to restore MariaDB after Runtime update failed")?;
    }
    return Err(error);
  }

  if was_running {
    if let Err(start_error) = restart_active_mariadb_runtime(state).await {
      let runtime_root = state.paths.runtimes.clone();
      rollback_online_runtime_install(
        &runtime_root,
        "mariadb",
        &downloaded.version,
        active_before.as_deref(),
      )
      .context("unable to roll back MariaDB after the updated Runtime failed to start")?;
      restart_active_mariadb_runtime(state)
        .await
        .context("unable to restart the previous MariaDB Runtime after update failed")?;
      return Err(start_error.context("updated MariaDB Runtime failed to restart"));
    }
  }
  Ok(())
}

async fn install_downloaded_mariadb_runtime_while_stopped(
  state: &AgentState,
  downloaded: &fabdev_updater::DownloadedRuntimeUpdate,
) -> Result<()> {
  let runtime_root = state.paths.runtimes.clone();
  let active_before = active_version(&runtime_root, "mariadb")?;
  let runtime_destination = runtime_root.join("mariadb").join(&downloaded.version);
  if runtime_destination.exists() {
    bail!(
      "Runtime mariadb {} is already installed",
      downloaded.version
    );
  }
  let artifact_path = downloaded.path.clone();
  let expected_sha256 = downloaded.sha256.clone();
  let version = downloaded.version.clone();
  let install_runtime_root = runtime_root.clone();
  let install_version = version.clone();
  let install_result = tokio::task::spawn_blocking(move || {
    install_tar_gz_with_health_check(
      artifact_path,
      &expected_sha256,
      "mariadb",
      &install_version,
      install_runtime_root,
      true,
      |staged| validate_staged_mariadb_runtime(staged, &install_version),
    )
  })
  .await
  .context("MariaDB Runtime installation task failed")?;
  if let Err(error) = install_result {
    rollback_online_runtime_install(&runtime_root, "mariadb", &version, active_before.as_deref())?;
    return Err(error.into());
  }
  if active_version(&runtime_root, "mariadb")?.as_deref() != Some(version.as_str()) {
    rollback_online_runtime_install(&runtime_root, "mariadb", &version, active_before.as_deref())?;
    bail!("online MariaDB Runtime installation did not activate the new version");
  }

  let apply_result = {
    let mut services = state.services.lock().await;
    services.set_mariadb_runtime(runtime_destination.clone());
    services.refresh_php_mariadb_connection().await
  };
  if let Err(error) = apply_result {
    rollback_online_runtime_install(&runtime_root, "mariadb", &version, active_before.as_deref())?;
    let restored_runtime = match active_before.as_deref() {
      Some(previous) => runtime_root.join("mariadb").join(previous),
      None => runtime_root.join("mariadb/current"),
    };
    let mut services = state.services.lock().await;
    services.set_mariadb_runtime(restored_runtime);
    services
      .refresh_php_mariadb_connection()
      .await
      .context("unable to restore MariaDB connection after Runtime update failed")?;
    return Err(error.context("unable to apply the installed MariaDB Runtime"));
  }
  Ok(())
}

async fn restart_active_mariadb_runtime(state: &AgentState) -> Result<()> {
  let runtime = active_mariadb_runtime_path(&state.paths.runtimes)?;
  let mut services = state.services.lock().await;
  services.set_mariadb_runtime(runtime);
  services.start_mariadb().await?;
  services.refresh_php_mariadb_connection().await
}

fn validate_staged_node_runtime(
  runtime: &Path,
  expected_version: &str,
) -> Result<(), RuntimeError> {
  let node = if cfg!(windows) {
    runtime.join("node.exe")
  } else {
    runtime.join("bin/node")
  };
  let npm = if cfg!(windows) {
    runtime.join("npm.cmd")
  } else {
    runtime.join("bin/npm")
  };
  if !node.is_file() || !npm.is_file() {
    return Err(RuntimeError::HealthCheckFailed(
      "required Node.js and npm launchers are missing".to_owned(),
    ));
  }
  let output = std::process::Command::new(&node)
    .arg("--version")
    .output()
    .map_err(|error| RuntimeError::HealthCheckFailed(error.to_string()))?;
  if !output.status.success()
    || String::from_utf8_lossy(&output.stdout).trim() != format!("v{expected_version}")
  {
    return Err(RuntimeError::HealthCheckFailed(format!(
      "Node.js did not report {expected_version}"
    )));
  }
  let output = std::process::Command::new(&npm)
    .arg("--version")
    .output()
    .map_err(|error| RuntimeError::HealthCheckFailed(error.to_string()))?;
  if !output.status.success() {
    return Err(RuntimeError::HealthCheckFailed(
      "npm health check failed".to_owned(),
    ));
  }
  Ok(())
}

fn validate_staged_mariadb_runtime(
  runtime: &Path,
  expected_version: &str,
) -> Result<(), RuntimeError> {
  let server = if cfg!(windows) {
    runtime.join("bin/mariadbd.exe")
  } else {
    runtime.join("bin/mariadbd")
  };
  let client = if cfg!(windows) {
    runtime.join("bin/mariadb.exe")
  } else {
    runtime.join("bin/mariadb")
  };
  for (label, binary) in [("MariaDB Server", server), ("MariaDB Client", client)] {
    if !binary.is_file() {
      return Err(RuntimeError::HealthCheckFailed(format!(
        "required {label} binary is missing"
      )));
    }
    let output = std::process::Command::new(&binary)
      .args(["--no-defaults", "--version"])
      .output()
      .map_err(|error| RuntimeError::HealthCheckFailed(error.to_string()))?;
    let reported = format!(
      "{}{}",
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() || !reported.contains(expected_version) {
      return Err(RuntimeError::HealthCheckFailed(format!(
        "{label} did not report {expected_version}"
      )));
    }
  }
  Ok(())
}

fn rollback_online_runtime_install(
  runtime_root: &Path,
  name: &str,
  version: &str,
  active_before: Option<&str>,
) -> Result<()> {
  match active_before {
    Some(previous) => set_active_version(runtime_root, name, previous)?,
    None => {
      deactivate_runtime(runtime_root, name)?;
    }
  }
  let destination = runtime_root.join(name).join(version);
  if destination.exists() {
    remove_online_runtime_directory(&destination)?;
  }
  Ok(())
}

fn restore_active_runtime(
  runtime_root: &Path,
  name: &str,
  active_before: Option<&str>,
) -> Result<()> {
  match active_before {
    Some(previous) => set_active_version(runtime_root, name, previous)?,
    None => {
      deactivate_runtime(runtime_root, name)?;
    }
  }
  Ok(())
}

fn validate_staged_php_runtime(runtime: &Path, expected_version: &str) -> Result<(), RuntimeError> {
  let cli = if cfg!(windows) {
    runtime.join("php.exe")
  } else {
    runtime.join("bin/php")
  };
  let server = if cfg!(windows) {
    runtime.join("php-cgi.exe")
  } else {
    runtime.join("sbin/php-fpm")
  };
  if !cli.is_file() || !server.is_file() {
    return Err(RuntimeError::HealthCheckFailed(
      "required PHP CLI and server binaries are missing".to_owned(),
    ));
  }
  let output = std::process::Command::new(&cli)
    .args(["-n", "--version"])
    .output()
    .map_err(|error| RuntimeError::HealthCheckFailed(error.to_string()))?;
  let version_prefix = format!("PHP {expected_version}");
  if !output.status.success()
    || !String::from_utf8_lossy(&output.stdout).starts_with(&version_prefix)
  {
    return Err(RuntimeError::HealthCheckFailed(format!(
      "PHP CLI did not report {expected_version}"
    )));
  }
  #[cfg(windows)]
  {
    let output = std::process::Command::new(&server)
      .args(["-n", "-v"])
      .output()
      .map_err(|error| RuntimeError::HealthCheckFailed(error.to_string()))?;
    if !output.status.success()
      || !String::from_utf8_lossy(&output.stdout).starts_with(&version_prefix)
    {
      return Err(RuntimeError::HealthCheckFailed(format!(
        "PHP CGI did not report {expected_version}"
      )));
    }
  }
  Ok(())
}

fn rollback_online_php_install(
  runtime_root: &Path,
  version: &str,
  active_before: Option<&str>,
  config_directory: &Path,
  config_existed: bool,
  service_directory: &Path,
  service_existed: bool,
) -> Result<()> {
  match active_before {
    Some(active_before) => set_active_version(runtime_root, "php", active_before)?,
    None => {
      deactivate_runtime(runtime_root, "php")?;
    }
  }
  remove_online_runtime_directory(&runtime_root.join("php").join(version))?;
  if !config_existed && config_directory.exists() {
    std::fs::remove_dir_all(config_directory)?;
  }
  if !service_existed && service_directory.exists() {
    std::fs::remove_dir_all(service_directory)?;
  }
  Ok(())
}

fn remove_online_runtime_directory(runtime: &Path) -> Result<()> {
  let metadata = std::fs::symlink_metadata(runtime)
    .with_context(|| format!("unable to inspect failed Runtime: {}", runtime.display()))?;
  if !metadata.is_dir() || metadata.file_type().is_symlink() {
    bail!(
      "failed Runtime path is not a directory: {}",
      runtime.display()
    );
  }
  std::fs::remove_dir_all(runtime)
    .with_context(|| format!("unable to remove failed Runtime: {}", runtime.display()))
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
  let current_os_version = current_runtime_os_version(platform)?;
  let mut installed_versions = HashMap::<String, HashSet<String>>::new();
  let mut active_versions = HashMap::<String, Option<String>>::new();
  let artifacts = catalog
    .catalog
    .runtimes
    .iter()
    .filter(|release| {
      release.platform == platform
        && release.architecture == architecture
        && current_os_version.as_ref().is_none_or(|current| {
          release
            .minimum_os_version
            .as_deref()
            .is_some_and(|minimum| runtime_os_version_supported(current, minimum))
        })
    })
    .map(|release| {
      if !installed_versions.contains_key(&release.name) {
        installed_versions.insert(
          release.name.clone(),
          list_installed_versions(runtime_root, &release.name)?
            .into_iter()
            .collect(),
        );
        active_versions.insert(
          release.name.clone(),
          active_version(runtime_root, &release.name)?,
        );
      }
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
        installed: installed_versions
          .get(&release.name)
          .is_some_and(|versions| versions.contains(&release.version)),
        active_version: active_versions.get(&release.name).cloned().flatten(),
      })
    })
    .collect::<Result<Vec<_>>>()?;
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

fn current_runtime_os_version(platform: &str) -> Result<Option<Vec<u16>>> {
  if platform != "macos" {
    return Ok(None);
  }
  let output = std::process::Command::new("/usr/bin/sw_vers")
    .arg("-productVersion")
    .output()
    .context("unable to read the current macOS version")?;
  if !output.status.success() {
    bail!("unable to read the current macOS version");
  }
  let version =
    String::from_utf8(output.stdout).context("the current macOS version is not valid UTF-8")?;
  parse_numeric_os_version(version.trim())
    .map(Some)
    .context("the current macOS version is invalid")
}

fn parse_numeric_os_version(version: &str) -> Result<Vec<u16>> {
  let parts = version
    .split('.')
    .map(|part| {
      if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("OS version must contain only numeric components");
      }
      part
        .parse::<u16>()
        .context("OS version component is too large")
    })
    .collect::<Result<Vec<_>>>()?;
  if parts.len() < 2 || parts.len() > 3 {
    bail!("OS version must contain two or three components");
  }
  Ok(parts)
}

fn runtime_os_version_supported(current: &[u16], minimum: &str) -> bool {
  let Ok(minimum) = parse_numeric_os_version(minimum) else {
    return false;
  };
  let length = current.len().max(minimum.len());
  for index in 0..length {
    match (current.get(index).copied().unwrap_or(0)).cmp(&minimum.get(index).copied().unwrap_or(0))
    {
      std::cmp::Ordering::Greater => return true,
      std::cmp::Ordering::Less => return false,
      std::cmp::Ordering::Equal => {}
    }
  }
  true
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

#[derive(Clone, Copy)]
enum TerminalPhpAction {
  Get,
  Enable,
  Disable,
}

fn terminal_php_response(paths: &AppPaths, action: TerminalPhpAction) -> AgentResponse {
  let result = match action {
    TerminalPhpAction::Get => terminal_php_state(&paths.root),
    TerminalPhpAction::Enable => enable_terminal_php(&paths.root),
    TerminalPhpAction::Disable => disable_terminal_php(&paths.root),
  };
  match result {
    Ok(state) => AgentResponse::TerminalPhp(TerminalPhpState {
      enabled: state.enabled,
      bin_path: state.bin_path,
      shim_path: state.shim_path,
    }),
    Err(error) => AgentResponse::Error {
      code: "terminal_php_integration_failed".to_owned(),
      message: error.to_string(),
    },
  }
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
  let data_root = state.paths.root.clone();
  tokio::task::spawn_blocking(move || build_node_runtime_state(&runtime_root, &data_root))
    .await
    .context("Node.js Runtime state task failed")?
}

fn build_node_runtime_state(runtime_root: &Path, data_root: &Path) -> Result<NodeRuntimeState> {
  let active_version = active_version(runtime_root, "node")?
    .filter(|version| node_runtime_binary(runtime_root, version).is_file());
  let installed = list_installed_versions(runtime_root, "node")?
    .into_iter()
    .filter(|version| node_runtime_binary(runtime_root, version).is_file())
    .map(|version| NodeRuntimeInfo {
      active: active_version.as_deref() == Some(version.as_str()),
      version,
    })
    .collect();
  let terminal = terminal_node_state(data_root)?;
  Ok(NodeRuntimeState {
    active_version,
    installed,
    terminal: TerminalNodeState {
      enabled: terminal.enabled,
      bin_path: terminal.bin_path,
      shim_paths: terminal.shim_paths,
    },
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

fn online_runtime_supported(name: &str, platform: &str, version: &str) -> bool {
  match (name, platform) {
    ("php", "windows") => php_series(version)
      .ok()
      .and_then(|series| series.parse::<PhpVersion>().ok())
      .is_some(),
    ("php", "macos") => version == "8.4.24",
    ("mariadb" | "node", "windows" | "macos") => {
      let parts = version.split('.').collect::<Vec<_>>();
      parts.len() == 3 && parts.iter().all(|part| part.parse::<u16>().is_ok())
    }
    _ => false,
  }
}

fn validate_php_release(release: &RuntimeRelease, artifact: &std::path::Path) -> Result<()> {
  if release.name != "php" {
    bail!("Runtime package must contain PHP, got {}", release.name);
  }
  let series = php_series(&release.version)?;
  let supported = if release.platform == "windows" {
    series.parse::<PhpVersion>().is_ok()
  } else {
    matches!(series.as_str(), "7.4" | "8.2" | "8.3" | "8.4")
  };
  if !supported {
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
  if !SUPPORTED_NODE_VERSIONS.contains(&release.version.as_str()) {
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

  #[test]
  fn installs_only_after_a_verified_download() {
    let operation_id = Uuid::new_v4();
    let task = RuntimeDownloadTask::new(RuntimeUpdateOperation {
      operation_id,
      status: RuntimeUpdateOperationStatus::Verified,
      name: "php".to_owned(),
      version: "8.4.24".to_owned(),
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      file_name: "php-8.4.24-macos-arm64-community.tar.gz".to_owned(),
      bytes_downloaded: 100,
      total_bytes: 100,
      sha256: "a".repeat(64),
      error: None,
    });

    assert_eq!(
      task.begin_install().expect("begin Runtime install").status,
      RuntimeUpdateOperationStatus::Installing
    );
    assert!(task.begin_install().is_err());
  }

  fn runtime_catalog_fixture(runtimes: Vec<RuntimeRelease>) -> ValidatedRuntimeCatalog {
    ValidatedRuntimeCatalog {
      catalog: fabdev_runtime::RuntimeCatalog {
        schema_version: fabdev_runtime::RUNTIME_CATALOG_SCHEMA_VERSION,
        product: fabdev_runtime::RUNTIME_CATALOG_PRODUCT.to_owned(),
        channel: fabdev_runtime::RUNTIME_CATALOG_CHANNEL.to_owned(),
        catalog_sequence: 1,
        generated_at: "2026-08-31T00:00:00Z".to_owned(),
        expires_at: "2027-02-27T00:00:00Z".to_owned(),
        unsigned_community_build: true,
        integrity: "sha256".to_owned(),
        compatibility: fabdev_runtime::RuntimeCatalogCompatibility {
          minimum_app_version: "0.1.11".to_owned(),
          minimum_agent_protocol_version: PROTOCOL_VERSION,
        },
        signature: None,
        runtimes,
      },
      sha256: "a".repeat(64),
    }
  }

  fn runtime_release_fixture(
    name: &str,
    version: &str,
    platform: &str,
    architecture: &str,
  ) -> RuntimeRelease {
    RuntimeRelease {
      name: name.to_owned(),
      version: version.to_owned(),
      platform: platform.to_owned(),
      architecture: architecture.to_owned(),
      minimum_os_version: Some(if platform == "macos" { "13.0" } else { "11.0" }.to_owned()),
      file_name: Some(format!(
        "{name}-{version}-{platform}-{architecture}-community.tar.gz"
      )),
      size: 100,
      sha256: "b".repeat(64),
      ..RuntimeRelease::default()
    }
  }

  #[test]
  fn filters_runtime_catalog_artifacts_for_the_current_desktop_target() {
    let root = std::env::temp_dir().join(format!("fabdev-agent-catalog-{}", Uuid::new_v4()));
    let (platform, architecture) = runtime_update_target().expect("read current Runtime target");
    let other_platform = if platform == "macos" {
      "windows"
    } else {
      "macos"
    };
    let other_architecture = if architecture == "arm64" {
      "x64"
    } else {
      "arm64"
    };
    let mut runtimes = Vec::new();
    for (name, version) in [
      ("php", "8.4.24"),
      ("mariadb", "12.3.2"),
      ("node", "20.20.2"),
      ("node", "24.20.0"),
    ] {
      runtimes.push(runtime_release_fixture(
        name,
        version,
        platform,
        architecture,
      ));
      runtimes.push(runtime_release_fixture(
        name,
        version,
        other_platform,
        other_architecture,
      ));
    }
    std::fs::create_dir_all(root.join("node/24.20.0")).expect("create installed Node.js fixture");
    set_active_version(&root, "node", "24.20.0").expect("set active Node.js fixture");

    let artifacts = runtime_update_artifacts(&runtime_catalog_fixture(runtimes), &root)
      .expect("build target Runtime artifacts");

    assert_eq!(artifacts.len(), 4);
    assert!(artifacts
      .iter()
      .all(|artifact| artifact.platform == platform && artifact.architecture == architecture));
    assert!(artifacts
      .iter()
      .find(|artifact| artifact.name == "node" && artifact.version == "24.20.0")
      .is_some_and(
        |artifact| artifact.installed && artifact.active_version.as_deref() == Some("24.20.0")
      ));
    std::fs::remove_dir_all(root).expect("remove Runtime Catalog fixture");
  }

  #[test]
  fn accepts_a_catalog_without_packages_for_the_current_target() {
    let root = std::env::temp_dir().join(format!("fabdev-agent-empty-catalog-{}", Uuid::new_v4()));
    let (platform, _) = runtime_update_target().expect("read current Runtime target");
    let release = if platform == "macos" {
      runtime_release_fixture("php", "8.4.24", "windows", "x64")
    } else {
      runtime_release_fixture("php", "8.4.24", "macos", "arm64")
    };

    let artifacts = runtime_update_artifacts(&runtime_catalog_fixture(vec![release]), &root)
      .expect("accept Catalog without current target packages");

    assert!(artifacts.is_empty());
  }

  #[cfg(unix)]
  #[test]
  fn validates_fixed_staged_php_binary_paths_and_version() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("fabdev-php-health-{}", Uuid::new_v4()));
    let cli = root.join("bin/php");
    let fpm = root.join("sbin/php-fpm");
    std::fs::create_dir_all(cli.parent().expect("CLI parent")).expect("create CLI parent");
    std::fs::create_dir_all(fpm.parent().expect("FPM parent")).expect("create FPM parent");
    std::fs::write(&cli, "#!/bin/sh\nprintf 'PHP 8.4.24 (cli)\\n'\n").expect("write CLI fixture");
    std::fs::write(&fpm, "fixture").expect("write FPM fixture");
    let mut permissions = std::fs::metadata(&cli)
      .expect("read CLI metadata")
      .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&cli, permissions).expect("make CLI executable");

    validate_staged_php_runtime(&root, "8.4.24").expect("validate staged PHP");
    let error = validate_staged_php_runtime(&root, "8.4.25").expect_err("reject wrong PHP");
    assert!(matches!(error, RuntimeError::HealthCheckFailed(_)));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rolls_back_only_the_new_online_runtime_and_restores_active_php() {
    let root = std::env::temp_dir().join(format!("fabdev-php-rollback-{}", Uuid::new_v4()));
    let runtimes = root.join("runtimes");
    let config = root.join("config/php/8.4");
    let service = root.join("services/php/8.4");
    std::fs::create_dir_all(runtimes.join("php/8.2.33")).expect("create existing Runtime");
    std::fs::create_dir_all(runtimes.join("php/8.4.24")).expect("create new Runtime");
    std::fs::create_dir_all(&config).expect("create new config");
    std::fs::create_dir_all(&service).expect("create new service config");
    set_active_version(&runtimes, "php", "8.4.24").expect("simulate changed active Runtime");

    rollback_online_php_install(
      &runtimes,
      "8.4.24",
      Some("8.2.33"),
      &config,
      false,
      &service,
      false,
    )
    .expect("roll back online Runtime");

    assert_eq!(
      active_version(&runtimes, "php").expect("read restored active Runtime"),
      Some("8.2.33".to_owned())
    );
    assert!(runtimes.join("php/8.2.33").is_dir());
    assert!(!runtimes.join("php/8.4.24").exists());
    assert!(!config.exists());
    assert!(!service.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rolls_back_mariadb_runtime_without_touching_data_config_or_logs() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-rollback-{}", Uuid::new_v4()));
    let runtimes = root.join("runtimes");
    let data = root.join("services/mariadb/data/customer.ibd");
    let config = root.join("config/mariadb/my.cnf");
    let log = root.join("logs/mariadb-process.log");
    for path in [&data, &config, &log] {
      std::fs::create_dir_all(path.parent().expect("state parent"))
        .expect("create MariaDB state directory");
      std::fs::write(path, path.to_string_lossy().as_bytes()).expect("write MariaDB state fixture");
    }
    std::fs::create_dir_all(runtimes.join("mariadb/12.2.2"))
      .expect("create previous MariaDB Runtime");
    std::fs::create_dir_all(runtimes.join("mariadb/12.3.2"))
      .expect("create updated MariaDB Runtime");
    set_active_version(&runtimes, "mariadb", "12.3.2").expect("activate updated MariaDB Runtime");

    rollback_online_runtime_install(&runtimes, "mariadb", "12.3.2", Some("12.2.2"))
      .expect("roll back MariaDB Runtime");

    assert_eq!(
      active_version(&runtimes, "mariadb").expect("read restored MariaDB Runtime"),
      Some("12.2.2".to_owned())
    );
    assert!(!runtimes.join("mariadb/12.3.2").exists());
    for path in [&data, &config, &log] {
      assert_eq!(
        std::fs::read(path).expect("read preserved MariaDB state"),
        path.to_string_lossy().as_bytes()
      );
    }
    std::fs::remove_dir_all(root).expect("remove MariaDB rollback fixture");
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
    let data_root = std::env::temp_dir().join(format!("fabdev-agent-node-{}", Uuid::new_v4()));
    let runtime_root = data_root.join("runtimes");
    let empty =
      build_node_runtime_state(&runtime_root, &data_root).expect("build empty Node.js state");
    assert_eq!(empty.active_version, None);
    assert!(empty.installed.is_empty());

    for version in SUPPORTED_NODE_VERSIONS {
      let binary = node_runtime_binary(&runtime_root, version);
      std::fs::create_dir_all(binary.parent().expect("Node.js binary parent"))
        .expect("create Node.js fixture");
      std::fs::write(binary, "fixture").expect("write Node.js fixture");
    }
    set_active_version(&runtime_root, "node", SUPPORTED_NODE_VERSIONS[1])
      .expect("activate Node.js fixture");
    let installed =
      build_node_runtime_state(&runtime_root, &data_root).expect("build Node.js state");
    assert_eq!(
      installed.active_version.as_deref(),
      Some(SUPPORTED_NODE_VERSIONS[1])
    );
    assert_eq!(installed.installed.len(), 2);
    assert!(installed
      .installed
      .iter()
      .any(|runtime| { runtime.version == SUPPORTED_NODE_VERSIONS[1] && runtime.active }));
    assert!(installed
      .installed
      .iter()
      .any(|runtime| { runtime.version == SUPPORTED_NODE_VERSIONS[0] && !runtime.active }));
    std::fs::remove_dir_all(data_root).expect("remove fixture");
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
  fn supports_online_runtimes_for_both_desktop_platforms() {
    assert!(online_runtime_supported("php", "windows", "7.4.33"));
    assert!(online_runtime_supported("php", "windows", "8.2.34"));
    assert!(online_runtime_supported("php", "windows", "9.1.2"));
    assert!(!online_runtime_supported("php", "windows", "6.4.1"));
    assert!(online_runtime_supported("php", "macos", "8.4.24"));
    assert!(!online_runtime_supported("php", "macos", "9.1.2"));
    assert!(online_runtime_supported("mariadb", "windows", "12.3.2"));
    assert!(online_runtime_supported("node", "windows", "20.20.2"));
    assert!(online_runtime_supported("node", "windows", "24.20.0"));
    assert!(online_runtime_supported("mariadb", "macos", "12.3.2"));
    assert!(online_runtime_supported("node", "macos", "20.20.2"));
    assert!(online_runtime_supported("node", "macos", "24.20.0"));
    assert!(!online_runtime_supported("node", "windows", "24.19"));
    assert!(!online_runtime_supported("node", "linux", "24.20.0"));
  }

  #[test]
  fn compares_runtime_minimum_macos_versions_numerically() {
    assert!(!runtime_os_version_supported(&[13, 4], "13.5"));
    assert!(runtime_os_version_supported(&[13, 5], "13.5"));
    assert!(runtime_os_version_supported(&[13, 5, 1], "13.5"));
    assert!(runtime_os_version_supported(&[14, 0], "13.5"));
    assert!(!runtime_os_version_supported(&[13, 5], "13.5.1"));
    assert!(!runtime_os_version_supported(&[13, 5], "13.x"));
  }

  #[test]
  fn rejects_invalid_numeric_os_versions() {
    assert!(parse_numeric_os_version("13.5").is_ok());
    assert!(parse_numeric_os_version("13.5.1").is_ok());
    assert!(parse_numeric_os_version("13").is_err());
    assert!(parse_numeric_os_version("13.5.1.2").is_err());
    assert!(parse_numeric_os_version("13.5-beta").is_err());
  }

  #[cfg(windows)]
  #[test]
  #[ignore = "requires the verified Windows Runtime packages built by the release workflow"]
  fn installs_real_windows_online_service_runtime_archives() {
    use sha2::{Digest, Sha256};

    let node_packages = [
      ("20.20.2", "FABDEV_WINDOWS_NODE20_RUNTIME_PACKAGE"),
      ("24.20.0", "FABDEV_WINDOWS_NODE24_RUNTIME_PACKAGE"),
    ];
    let data_root =
      std::env::temp_dir().join(format!("fabdev-online-service-runtimes-{}", Uuid::new_v4()));
    let root = data_root.join("runtimes");

    for (version, variable) in node_packages {
      let artifact = PathBuf::from(
        std::env::var(variable)
          .unwrap_or_else(|_| panic!("{variable} must identify the release package")),
      );
      let checksum = hex::encode(Sha256::digest(
        std::fs::read(&artifact).expect("read Windows Node.js Runtime package"),
      ));
      install_tar_gz_with_health_check(
        &artifact,
        &checksum,
        "node",
        version,
        &root,
        false,
        |staged| validate_staged_node_runtime(staged, version),
      )
      .expect("install and validate packaged Windows Node.js Runtime");
    }
    assert_eq!(
      active_version(&root, "node").expect("read inactive Node.js Runtime"),
      None
    );
    set_active_version(&root, "node", "20.20.2").expect("select Node.js 20");
    let terminal = enable_terminal_node(&data_root).expect("enable terminal Node.js");
    let node_shim = terminal.bin_path.join("node.cmd");
    let node20 = std::process::Command::new("cmd.exe")
      .args(["/d", "/c"])
      .arg(&node_shim)
      .arg("--version")
      .output()
      .expect("run Node.js 20 shim");
    assert!(String::from_utf8_lossy(&node20.stdout).contains("v20.20.2"));
    set_active_version(&root, "node", "24.20.0").expect("switch to Node.js 24");
    let node24 = std::process::Command::new("cmd.exe")
      .args(["/d", "/c"])
      .arg(&node_shim)
      .arg("--version")
      .output()
      .expect("run Node.js 24 shim");
    assert!(String::from_utf8_lossy(&node24.stdout).contains("v24.20.0"));
    assert_eq!(
      active_version(&root, "node").expect("read active Node.js Runtime"),
      Some("24.20.0".to_owned())
    );

    if let Ok(path) = std::env::var("FABDEV_WINDOWS_MARIADB_RUNTIME_PACKAGE") {
      let mariadb_artifact = PathBuf::from(path);
      let mariadb_checksum = hex::encode(Sha256::digest(
        std::fs::read(&mariadb_artifact).expect("read Windows MariaDB Runtime package"),
      ));
      install_tar_gz_with_health_check(
        &mariadb_artifact,
        &mariadb_checksum,
        "mariadb",
        "12.3.2",
        &root,
        true,
        |staged| validate_staged_mariadb_runtime(staged, "12.3.2"),
      )
      .expect("install and validate packaged Windows MariaDB Runtime");
      assert_eq!(
        active_version(&root, "mariadb").expect("read active MariaDB Runtime"),
        Some("12.3.2".to_owned())
      );
    }
    disable_terminal_node(&data_root).expect("disable terminal Node.js");
    std::fs::remove_dir_all(data_root).expect("remove Windows Runtime fixture");
  }

  #[cfg(target_os = "macos")]
  #[test]
  #[ignore = "requires verified macOS Runtime packages built by the release preparation script"]
  fn installs_real_macos_online_runtime_archives() {
    use sha2::{Digest, Sha256};

    let packages = [
      ("php", "8.4.24", "FABDEV_MACOS_PHP_RUNTIME_PACKAGE", false),
      (
        "node",
        "20.20.2",
        "FABDEV_MACOS_NODE20_RUNTIME_PACKAGE",
        false,
      ),
      (
        "node",
        "24.20.0",
        "FABDEV_MACOS_NODE24_RUNTIME_PACKAGE",
        false,
      ),
      (
        "mariadb",
        "12.3.2",
        "FABDEV_MACOS_MARIADB_RUNTIME_PACKAGE",
        true,
      ),
    ];
    let data_root =
      std::env::temp_dir().join(format!("fabdev-online-macos-runtimes-{}", Uuid::new_v4()));
    let runtime_root = data_root.join("runtimes");

    for (name, version, variable, activate) in packages {
      let artifact = PathBuf::from(
        std::env::var(variable)
          .unwrap_or_else(|_| panic!("{variable} must identify the release package")),
      );
      let checksum = hex::encode(Sha256::digest(
        std::fs::read(&artifact).expect("read macOS Runtime package"),
      ));
      install_tar_gz_with_health_check(
        &artifact,
        &checksum,
        name,
        version,
        &runtime_root,
        activate,
        |staged| match name {
          "php" => validate_staged_php_runtime(staged, version),
          "node" => validate_staged_node_runtime(staged, version),
          "mariadb" => validate_staged_mariadb_runtime(staged, version),
          _ => unreachable!("fixed macOS Runtime fixture"),
        },
      )
      .unwrap_or_else(|error| panic!("install and validate macOS {name} {version}: {error}"));
    }

    assert_eq!(
      active_version(&runtime_root, "php").expect("read inactive PHP Runtime"),
      None
    );
    assert_eq!(
      active_version(&runtime_root, "node").expect("read inactive Node.js Runtime"),
      None
    );
    assert_eq!(
      active_version(&runtime_root, "mariadb").expect("read active MariaDB Runtime"),
      Some("12.3.2".to_owned())
    );
    std::fs::remove_dir_all(data_root).expect("remove macOS Runtime fixture");
  }

  #[cfg(target_os = "macos")]
  #[tokio::test]
  #[ignore = "requires a verified macOS PHP Runtime package"]
  async fn installs_real_macos_php_through_the_online_agent_protocol() {
    use sha2::{Digest, Sha256};

    struct FixtureDirectory(PathBuf);

    impl Drop for FixtureDirectory {
      fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
      }
    }

    let artifact = PathBuf::from(
      std::env::var("FABDEV_MACOS_PHP_RUNTIME_PACKAGE")
        .expect("FABDEV_MACOS_PHP_RUNTIME_PACKAGE must identify the release package"),
    );
    let fixture = FixtureDirectory(
      PathBuf::from("/private/tmp").join(format!("fabdev-online-php-{}", Uuid::new_v4())),
    );
    let paths = AppPaths::from_root(&fixture.0);
    paths.ensure().expect("create online PHP fixture paths");

    let catalog_contents =
      fabdev_runtime::generate_community_php_catalog(&fabdev_runtime::CommunityPhpCatalogInput {
        release_version: "0.1.11",
        catalog_sequence: 902,
        generated_at: "2026-01-01T00:00:00Z",
        expires_at: "2099-01-01T00:00:00Z",
        minimum_app_version: env!("CARGO_PKG_VERSION"),
        macos_arm64_package: Some(&artifact),
        windows_x64_package: None,
        now_unix_seconds: std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .expect("read current time")
          .as_secs() as i64,
      })
      .expect("generate real PHP Runtime Catalog");
    let catalog_sha256 = hex::encode(Sha256::digest(&catalog_contents));
    let update_root = paths.cache.join("runtime-updates");
    let pending_root = update_root.join("pending");
    std::fs::create_dir_all(&pending_root).expect("create Runtime pending cache");
    std::fs::write(
      update_root.join("fabdev-runtime-v1.json"),
      &catalog_contents,
    )
    .expect("cache accepted Runtime Catalog");
    std::fs::write(
      update_root.join("accepted-catalog.json"),
      format!("{{\n  \"sequence\": 902,\n  \"sha256\": \"{catalog_sha256}\"\n}}\n"),
    )
    .expect("cache accepted Runtime Catalog state");
    let file_name = "php-8.4.24-macos-arm64-community.tar.gz";
    let pending_package = pending_root.join(file_name);
    std::fs::copy(&artifact, &pending_package).expect("stage verified PHP Runtime package");

    std::fs::create_dir_all(paths.runtimes.join("php/8.2.33"))
      .expect("create existing active PHP fixture");
    set_active_version(&paths.runtimes, "php", "8.2.33").expect("set existing active PHP fixture");

    let repository = SiteRepository::open(paths.database()).expect("open Site fixture database");
    let state = AgentState {
      paths: paths.clone(),
      sites: Mutex::new(repository),
      services: Mutex::new(ServiceSupervisor::new(
        paths.clone(),
        RuntimePaths::from_runtime_root(&paths.runtimes),
        ServicePorts {
          dns: 53_535,
          http: 8_080,
          https: 8_443,
          mariadb: 3_306,
        },
      )),
      lan_share: Mutex::new(LanShareState::new(8_080)),
      proxy_manager: Mutex::new(ProxyManager::new(Vec::new()).expect("create Proxy fixture")),
      runtime_updates: RuntimeUpdateManager::default(),
      shutdown: Arc::new(Notify::new()),
    };
    let artifact = cached_runtime_update_artifact(&state, "php", "8.4.24")
      .await
      .expect("load accepted PHP Runtime artifact");
    let operation = state
      .runtime_updates
      .start(paths.cache.clone(), artifact.clone())
      .await
      .expect("start cached PHP Runtime verification");
    let mut verified = None;
    for _ in 0..200 {
      let snapshot = state
        .runtime_updates
        .get(operation.operation_id)
        .await
        .expect("read Runtime verification progress");
      if snapshot.status == RuntimeUpdateOperationStatus::Verified {
        verified = Some(snapshot);
        break;
      }
      if snapshot.status == RuntimeUpdateOperationStatus::Failed {
        panic!("real PHP Runtime verification failed: {:?}", snapshot.error);
      }
      tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let verified = verified.expect("real PHP Runtime verification timed out");
    assert_eq!(verified.bytes_downloaded, verified.total_bytes);

    let response = handle_request(
      AgentRequest::InstallDownloadedRuntime {
        operation_id: operation.operation_id,
      },
      &state,
    )
    .await;
    let AgentResponse::RuntimeUpdateOperation(installed) = response else {
      panic!("unexpected online PHP install response: {response:?}");
    };
    assert_eq!(
      installed.status,
      RuntimeUpdateOperationStatus::Completed,
      "online PHP install failed: {:?}",
      installed.error
    );
    assert_eq!(installed.error, None);
    assert!(paths.runtimes.join("php/8.4.24/bin/php").is_file());
    assert_eq!(
      active_version(&paths.runtimes, "php").expect("read preserved active PHP"),
      Some("8.2.33".to_owned())
    );

    remove_installed_version(&paths.runtimes, "php", "8.4.24")
      .expect("remove successful PHP fixture before failure path");
    std::fs::write(&pending_package, b"tampered").expect("tamper cached PHP Runtime package");
    let tampered_operation_id = Uuid::new_v4();
    let tampered_task = Arc::new(RuntimeDownloadTask::new(RuntimeUpdateOperation {
      operation_id: tampered_operation_id,
      status: RuntimeUpdateOperationStatus::Verified,
      name: artifact.name,
      version: artifact.version,
      platform: artifact.platform,
      architecture: artifact.architecture,
      file_name: artifact.file_name,
      bytes_downloaded: artifact.size,
      total_bytes: artifact.size,
      sha256: artifact.sha256,
      error: None,
    }));
    state
      .runtime_updates
      .operations
      .lock()
      .await
      .insert(tampered_operation_id, tampered_task);
    let response = handle_request(
      AgentRequest::InstallDownloadedRuntime {
        operation_id: tampered_operation_id,
      },
      &state,
    )
    .await;
    let AgentResponse::RuntimeUpdateOperation(rejected) = response else {
      panic!("unexpected tampered PHP install response: {response:?}");
    };
    assert_eq!(rejected.status, RuntimeUpdateOperationStatus::Failed);
    assert!(rejected
      .error
      .as_deref()
      .is_some_and(|error| error.contains("size does not match")));
    assert!(!paths.runtimes.join("php/8.4.24").exists());
    assert_eq!(
      active_version(&paths.runtimes, "php").expect("read active PHP after rejection"),
      Some("8.2.33".to_owned())
    );

    drop(state);
  }

  #[test]
  fn validates_only_the_supported_node_runtime_releases() {
    let root = std::env::temp_dir().join(format!("fabdev-node-release-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, "fixture").expect("write artifact");
    let mut release = RuntimeRelease {
      name: "node".to_owned(),
      version: SUPPORTED_NODE_VERSIONS[0].to_owned(),
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

    validate_node_release(&release, &artifact).expect("accept Node.js 20 package");
    release.version = SUPPORTED_NODE_VERSIONS[1].to_owned();
    validate_node_release(&release, &artifact).expect("accept Node.js 24 package");
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
