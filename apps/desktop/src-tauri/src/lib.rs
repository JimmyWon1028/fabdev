use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{bail, Context};
#[cfg(any(target_os = "macos", windows))]
use fabdev_core::{create_site, SiteInput, SiteRepository};
use fabdev_core::{
  normalize_domain, AgentEndpoint, AgentRequest, AgentResponse, AgentStatus, AppPaths, PhpVersion,
  ServiceState, PROTOCOL_VERSION,
};
#[cfg(any(target_os = "macos", windows))]
use fabdev_runtime::{
  active_version, is_runtime_marked_removed, list_installed_versions, set_active_version,
};
#[cfg(target_os = "macos")]
use fabdev_runtime::{install_tar_gz_with_activation, RuntimeRelease};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
#[cfg(unix)]
use tokio::net::UnixStream;

const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray/fabdev-tray-44.png");
#[cfg(target_os = "macos")]
const MACOS_APP_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
const SERVICE_STATE_CHANGED_EVENT: &str = "fabdev://service-state-changed";
const AGENT_ERROR_EVENT: &str = "fabdev://agent-error";
const APP_QUIT_STARTED_EVENT: &str = "fabdev://quit-started";
const APP_QUIT_FAILED_EVENT: &str = "fabdev://quit-failed";
const AGENT_START_TIMEOUT: Duration = Duration::from_secs(5);
const AGENT_START_POLL_INTERVAL: Duration = Duration::from_millis(50);
const AGENT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT_INSTALL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SYSTEM_INGRESS_ERROR_PREFIX: &str = "system ingress is unavailable on DNS port ";

static AGENT_START_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static QUIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static EXIT_ALLOWED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BundledRuntimeSpec {
  name: &'static str,
  version: &'static str,
}

#[cfg(target_os = "macos")]
const BUNDLED_MACOS_RUNTIMES: [BundledRuntimeSpec; 4] = [
  BundledRuntimeSpec {
    name: "dnsmasq",
    version: "2.93",
  },
  BundledRuntimeSpec {
    name: "nginx",
    version: "1.30.4",
  },
  BundledRuntimeSpec {
    name: "php",
    version: "7.4.33",
  },
  BundledRuntimeSpec {
    name: "php",
    version: "8.2.33",
  },
];

struct TrayMenuItems {
  service_toggle: MenuItem<tauri::Wry>,
  service_state: std::sync::Mutex<TrayServiceState>,
  mariadb_toggle: MenuItem<tauri::Wry>,
  mariadb_state: std::sync::Mutex<TrayMariaDbState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayServiceState {
  Running,
  Stopped,
  Mixed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayMariaDbState {
  Running,
  Stopped,
  Busy,
  NotInstalled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayActionTarget {
  Web,
  MariaDb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayAction {
  Open,
  ToggleAll,
  ToggleMariaDb,
  Quit,
}

impl TrayAction {
  fn from_id(id: &str) -> Option<Self> {
    match id {
      "open-fabdev" => Some(Self::Open),
      "toggle-all" => Some(Self::ToggleAll),
      "toggle-mariadb" => Some(Self::ToggleMariaDb),
      "quit-fabdev" | "quit-fabdev-app" => Some(Self::Quit),
      _ => None,
    }
  }
}

#[tauri::command]
async fn agent_request(app: AppHandle, request: AgentRequest) -> Result<AgentResponse, String> {
  let response = request_agent_with_ingress_repair(request)
    .await
    .map_err(|error| error.to_string())?;
  update_tray_from_response(&app, &response);
  Ok(response)
}

const CONFIG_TRANSFER_MAX_BYTES: usize = 4 * 1024 * 1024;

#[tauri::command]
fn read_config_transfer_file(path: String) -> Result<String, String> {
  let path = PathBuf::from(path);
  validate_config_transfer_path(&path)?;
  let metadata =
    std::fs::metadata(&path).map_err(|error| format!("unable to inspect import file: {error}"))?;
  if metadata.len() > CONFIG_TRANSFER_MAX_BYTES as u64 {
    return Err("import file exceeds the 4 MiB limit".to_owned());
  }
  std::fs::read_to_string(&path).map_err(|error| format!("unable to read import file: {error}"))
}

#[tauri::command]
fn write_config_transfer_file(path: String, contents: String) -> Result<(), String> {
  let path = PathBuf::from(path);
  validate_config_transfer_path(&path)?;
  if contents.len() > CONFIG_TRANSFER_MAX_BYTES {
    return Err("export file exceeds the 4 MiB limit".to_owned());
  }
  serde_json::from_str::<serde_json::Value>(&contents)
    .map_err(|error| format!("unable to export invalid JSON: {error}"))?;
  std::fs::write(&path, contents).map_err(|error| format!("unable to write export file: {error}"))
}

fn validate_config_transfer_path(path: &Path) -> Result<(), String> {
  if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
    return Err("fabDev configuration files must use the .json extension".to_owned());
  }
  Ok(())
}

#[tauri::command]
fn reveal_php_ini(php_version: PhpVersion) -> Result<String, String> {
  let paths =
    AppPaths::discover().ok_or_else(|| "unable to locate fabDev application data".to_owned())?;
  let path = php_ini_path(&paths, &php_version);
  if !path.is_file() {
    return Err(format!("php.ini does not exist: {}", path.display()));
  }
  reveal_path(&path).map_err(|error| error.to_string())?;
  Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn reveal_default_php_ini() -> Result<String, String> {
  let paths =
    AppPaths::discover().ok_or_else(|| "unable to locate fabDev application data".to_owned())?;
  let path = default_php_ini_path(&paths);
  if !path.is_file() {
    return Err(format!(
      "default php.ini does not exist: {}",
      path.display()
    ));
  }
  reveal_path(&path).map_err(|error| error.to_string())?;
  Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_site(domain: String, secured: bool) -> Result<(), String> {
  let url = site_url(&domain, secured)?;
  open_url(&url).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_proxy_in_chrome(domain: String, listen_port: u16) -> Result<(), String> {
  let url = proxy_url(&domain, listen_port)?;
  open_url_in_chrome(&url).map_err(|error| error.to_string())
}

fn site_url(domain: &str, secured: bool) -> Result<String, String> {
  normalize_domain(domain)
    .map(|domain| {
      let scheme = if secured { "https" } else { "http" };
      format!("{scheme}://{domain}")
    })
    .map_err(|error| error.to_string())
}

fn proxy_url(domain: &str, listen_port: u16) -> Result<String, String> {
  if !(1024..=65535).contains(&listen_port) {
    return Err("Proxy port must be between 1024 and 65535".to_owned());
  }
  normalize_domain(domain)
    .map(|domain| format!("http://{domain}:{listen_port}/"))
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn trust_local_ca(certificate_path: String) -> Result<(), String> {
  let paths =
    AppPaths::discover().ok_or_else(|| "unable to locate fabDev application data".to_owned())?;
  let expected = paths.config.join("tls/ca.crt");
  let requested = std::fs::canonicalize(&certificate_path)
    .map_err(|error| format!("unable to resolve local CA certificate: {error}"))?;
  let expected = std::fs::canonicalize(&expected)
    .map_err(|error| format!("fabDev local CA certificate is missing: {error}"))?;
  if requested != expected {
    return Err("refusing to trust a certificate outside fabDev managed storage".to_owned());
  }
  trust_local_ca_for_platform(&requested).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn trust_local_ca_for_platform(certificate_path: &Path) -> anyhow::Result<()> {
  let home = std::env::var_os("HOME").context("HOME is not defined")?;
  let login_keychain = PathBuf::from(home).join("Library/Keychains/login.keychain-db");
  if !login_keychain.is_file() {
    bail!(
      "macOS Login keychain is missing: {}",
      login_keychain.display()
    );
  }
  let already_trusted = Command::new("/usr/bin/security")
    .arg("verify-cert")
    .arg("-c")
    .arg(certificate_path)
    .arg("-l")
    .arg("-k")
    .arg(&login_keychain)
    .arg("-L")
    .arg("-q")
    .status()
    .context("unable to inspect the macOS Login keychain")?;
  if already_trusted.success() {
    return Ok(());
  }
  let output = Command::new("/usr/bin/security")
    .arg("add-trusted-cert")
    .arg("-r")
    .arg("trustRoot")
    .arg("-k")
    .arg(&login_keychain)
    .arg(certificate_path)
    .output()
    .context("unable to update the macOS Login keychain")?;
  if !output.status.success() {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    bail!("unable to trust the fabDev local CA: {detail}");
  }
  Ok(())
}

#[cfg(windows)]
fn trust_local_ca_for_platform(certificate_path: &Path) -> anyhow::Result<()> {
  let executable = std::env::current_exe().context("unable to locate fabDev Desktop")?;
  let helper = executable
    .parent()
    .context("fabDev Desktop executable has no parent directory")?
    .join("fabdev-windows-helper.exe");
  let status = Command::new(&helper)
    .arg("trust-ca")
    .arg("--certificate")
    .arg(certificate_path)
    .status()
    .with_context(|| format!("unable to start Windows Helper at {}", helper.display()))?;
  if !status.success() {
    bail!("fabDev Windows Helper could not trust the local CA");
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn trust_local_ca_for_platform(_certificate_path: &Path) -> anyhow::Result<()> {
  bail!("local CA trust is not supported on this platform")
}

fn php_ini_path(paths: &AppPaths, php_version: &PhpVersion) -> PathBuf {
  paths
    .config
    .join("php")
    .join(php_version.to_string())
    .join("php.ini")
}

fn default_php_ini_path(paths: &AppPaths) -> PathBuf {
  paths.config.join("php/default/php.ini")
}

#[cfg(any(target_os = "macos", windows))]
fn initialize_empty_php_ini_for_runtime(paths: &AppPaths, version: &str) -> anyhow::Result<()> {
  let parts = version.split('.').collect::<Vec<_>>();
  if parts.len() != 3 || parts.iter().any(|part| part.parse::<u16>().is_err()) {
    bail!("invalid bundled PHP Runtime version: {version}");
  }
  let php_version = format!("{}.{}", parts[0], parts[1])
    .parse::<PhpVersion>()
    .context("invalid bundled PHP Runtime series")?;
  let path = php_ini_path(paths, &php_version);
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(path, "")?;
  Ok(())
}

#[cfg(target_os = "macos")]
fn reveal_path(path: &Path) -> anyhow::Result<()> {
  let status = Command::new("/usr/bin/open").arg("-R").arg(path).status()?;
  if !status.success() {
    bail!("Finder could not reveal {}", path.display());
  }
  Ok(())
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> anyhow::Result<()> {
  let status = Command::new("/usr/bin/open").arg(url).status()?;
  if !status.success() {
    bail!("default browser could not open {url}");
  }
  Ok(())
}

#[cfg(target_os = "macos")]
fn open_url_in_chrome(url: &str) -> anyhow::Result<()> {
  let status = Command::new("/usr/bin/open")
    .args(["-a", "Google Chrome", url])
    .status()?;
  if !status.success() {
    bail!("Google Chrome could not open {url}");
  }
  Ok(())
}

#[cfg(target_os = "windows")]
fn reveal_path(path: &Path) -> anyhow::Result<()> {
  let status = Command::new("explorer.exe")
    .arg(format!("/select,{}", path.display()))
    .status()?;
  if !status.success() {
    bail!("File Explorer could not reveal {}", path.display());
  }
  Ok(())
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) -> anyhow::Result<()> {
  let status = Command::new("rundll32.exe")
    .arg("url.dll,FileProtocolHandler")
    .arg(url)
    .status()?;
  if !status.success() {
    bail!("default browser could not open {url}");
  }
  Ok(())
}

#[cfg(target_os = "windows")]
fn open_url_in_chrome(url: &str) -> anyhow::Result<()> {
  let roots = ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"];
  let chrome = roots
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .map(|root| root.join("Google/Chrome/Application/chrome.exe"))
    .find(|path| path.is_file())
    .context("Google Chrome is not installed in a standard location")?;
  let status = Command::new(chrome).arg(url).status()?;
  if !status.success() {
    bail!("Google Chrome could not open {url}");
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_path(path: &Path) -> anyhow::Result<()> {
  let parent = path.parent().context("php.ini has no parent directory")?;
  let status = Command::new("xdg-open").arg(parent).status()?;
  if !status.success() {
    bail!("file manager could not open {}", parent.display());
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_url(url: &str) -> anyhow::Result<()> {
  let status = Command::new("xdg-open").arg(url).status()?;
  if !status.success() {
    bail!("default browser could not open {url}");
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_url_in_chrome(url: &str) -> anyhow::Result<()> {
  for executable in [
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
  ] {
    if Command::new(executable)
      .arg(url)
      .status()
      .is_ok_and(|status| status.success())
    {
      return Ok(());
    }
  }
  bail!("Google Chrome or Chromium is not installed")
}

async fn request_agent(request: AgentRequest) -> anyhow::Result<AgentResponse> {
  let paths = AppPaths::discover()
    .ok_or_else(|| anyhow::anyhow!("unable to locate fabDev application data"))?;
  let endpoint = paths.agent_endpoint();
  ensure_agent_running(&paths).await?;
  send_request(&endpoint, request)
    .await
    .context("fabDev Agent did not accept the request")
}

async fn request_agent_with_ingress_repair(request: AgentRequest) -> anyhow::Result<AgentResponse> {
  let can_repair_ingress = matches!(request, AgentRequest::StartAll);
  let response = request_agent(request.clone()).await?;
  if !can_repair_ingress || !is_system_ingress_error(&response) {
    return Ok(response);
  }

  restart_system_helper().await?;
  request_agent(request)
    .await
    .context("fabDev System Helper restarted but services did not start")
}

fn is_system_ingress_error(response: &AgentResponse) -> bool {
  matches!(
    response,
    AgentResponse::Error { message, .. } if message.starts_with(SYSTEM_INGRESS_ERROR_PREFIX)
  )
}

#[cfg(target_os = "macos")]
async fn restart_system_helper() -> anyhow::Result<()> {
  const RESTART_SCRIPT: &str =
    "do shell script \"/bin/launchctl kickstart -k system/com.fabdev.system-helper\" with administrator privileges";

  let output = tokio::task::spawn_blocking(|| {
    Command::new("/usr/bin/osascript")
      .arg("-e")
      .arg(RESTART_SCRIPT)
      .output()
  })
  .await
  .context("System Helper restart task failed")??;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    bail!(
      "unable to restart fabDev System Helper{}",
      if stderr.is_empty() {
        String::new()
      } else {
        format!(": {stderr}")
      }
    );
  }

  tokio::time::sleep(Duration::from_millis(500)).await;
  Ok(())
}

#[cfg(not(target_os = "macos"))]
async fn restart_system_helper() -> anyhow::Result<()> {
  bail!("fabDev System Helper restart is only available on macOS")
}

async fn ensure_agent_running(paths: &AppPaths) -> anyhow::Result<()> {
  let lock = AGENT_START_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
  let _guard = lock.lock().await;
  let endpoint = paths.agent_endpoint();
  let restart_services = match agent_protocol_version(&endpoint).await {
    Ok(PROTOCOL_VERSION) => return Ok(()),
    Ok(actual) => shutdown_incompatible_agent(&endpoint, actual).await?,
    Err(_) => ServicesToRestart::default(),
  };

  #[cfg(unix)]
  {
    let AgentEndpoint::UnixSocket(socket) = &endpoint;
    remove_stale_agent_socket(socket)?;
  }
  paths
    .ensure()
    .context("unable to create fabDev application data directories")?;
  let executable = resolve_agent_executable()?;
  spawn_agent(&executable, paths)?;

  let started = tokio::time::Instant::now();
  while started.elapsed() < AGENT_START_TIMEOUT {
    if matches!(
      agent_protocol_version(&endpoint).await,
      Ok(version) if version == PROTOCOL_VERSION
    ) {
      if restart_services.web {
        match send_request(&endpoint, AgentRequest::StartAll).await? {
          AgentResponse::Started => {}
          AgentResponse::Error { message, .. } => {
            bail!("fabDev Agent upgraded but services could not restart: {message}")
          }
          _ => bail!("fabDev Agent returned an unexpected service restart response"),
        }
      }
      if restart_services.mariadb {
        match send_request(&endpoint, AgentRequest::StartMariaDb).await? {
          AgentResponse::MariaDbStarted => {}
          AgentResponse::Error { message, .. } => {
            bail!("fabDev Agent upgraded but MariaDB could not restart: {message}")
          }
          _ => bail!("fabDev Agent returned an unexpected MariaDB restart response"),
        }
      }
      return Ok(());
    }
    tokio::time::sleep(AGENT_START_POLL_INTERVAL).await;
  }
  bail!(
    "fabDev Agent did not become ready within {} seconds; see {}",
    AGENT_START_TIMEOUT.as_secs(),
    paths.logs.join("agent-process.log").display()
  )
}

async fn agent_protocol_version(endpoint: &AgentEndpoint) -> anyhow::Result<u16> {
  match send_request(endpoint, AgentRequest::Ping).await? {
    AgentResponse::Pong { protocol_version } => Ok(protocol_version),
    _ => bail!("fabDev Agent returned an unexpected ping response"),
  }
}

async fn shutdown_incompatible_agent(
  endpoint: &AgentEndpoint,
  actual_protocol_version: u16,
) -> anyhow::Result<ServicesToRestart> {
  let restart_services = match send_request(endpoint, AgentRequest::GetStatus).await {
    Ok(AgentResponse::Status(status)) => services_to_restart(&status),
    _ => ServicesToRestart::default(),
  };
  match send_request(endpoint, AgentRequest::Shutdown).await? {
    AgentResponse::Stopped => {}
    AgentResponse::Error { message, .. } => {
      bail!(
        "fabDev Agent protocol {actual_protocol_version} cannot upgrade automatically: {message}"
      )
    }
    _ => bail!("fabDev Agent returned an unexpected shutdown response"),
  }

  let started = tokio::time::Instant::now();
  while started.elapsed() < AGENT_START_TIMEOUT {
    if agent_protocol_version(endpoint).await.is_err() {
      return Ok(restart_services);
    }
    tokio::time::sleep(AGENT_START_POLL_INTERVAL).await;
  }
  bail!("incompatible fabDev Agent did not stop during automatic upgrade")
}

async fn shutdown_agent_before_exit() -> anyhow::Result<()> {
  let Some(paths) = AppPaths::discover() else {
    return Ok(());
  };
  let endpoint = paths.agent_endpoint();
  shutdown_agent_at(&endpoint).await
}

async fn shutdown_agent_at(endpoint: &AgentEndpoint) -> anyhow::Result<()> {
  if agent_protocol_version(endpoint).await.is_err() {
    #[cfg(unix)]
    {
      let AgentEndpoint::UnixSocket(socket) = endpoint;
      remove_stale_agent_socket(socket)?;
    }
    return Ok(());
  }

  match send_request(endpoint, AgentRequest::StopAll).await {
    Ok(AgentResponse::Stopped) | Ok(AgentResponse::Error { .. }) => {}
    Ok(_) => bail!("fabDev Agent returned an unexpected Web service stop response"),
    Err(error) => return Err(error).context("unable to stop Web services before Desktop quit"),
  }
  match send_request(endpoint, AgentRequest::Shutdown).await {
    Ok(AgentResponse::Stopped) => {}
    Ok(AgentResponse::Error { message, .. }) => {
      bail!("fabDev Agent could not stop before Desktop quit: {message}")
    }
    Ok(_) => bail!("fabDev Agent returned an unexpected shutdown response"),
    Err(_) if agent_protocol_version(endpoint).await.is_err() => return Ok(()),
    Err(error) => return Err(error).context("unable to stop fabDev Agent before Desktop quit"),
  }

  let started = tokio::time::Instant::now();
  while started.elapsed() < AGENT_START_TIMEOUT {
    if agent_protocol_version(endpoint).await.is_err() {
      #[cfg(unix)]
      {
        let AgentEndpoint::UnixSocket(socket) = endpoint;
        remove_stale_agent_socket(socket)?;
      }
      return Ok(());
    }
    tokio::time::sleep(AGENT_START_POLL_INTERVAL).await;
  }
  bail!("fabDev Agent did not stop before Desktop quit")
}

fn request_app_quit(app: AppHandle) {
  if QUIT_IN_PROGRESS.swap(true, Ordering::SeqCst) {
    return;
  }
  set_tray_all_busy(&app);
  show_main_window(&app);
  let _ = app.emit(APP_QUIT_STARTED_EVENT, ());
  tauri::async_runtime::spawn(async move {
    match shutdown_agent_before_exit().await {
      Ok(()) => {
        EXIT_ALLOWED.store(true, Ordering::SeqCst);
        app.exit(0);
      }
      Err(error) => {
        QUIT_IN_PROGRESS.store(false, Ordering::SeqCst);
        let _ = app.emit(APP_QUIT_FAILED_EVENT, ());
        let _ = app.emit(AGENT_ERROR_EVENT, error.to_string());
        refresh_tray_service_state(&app).await;
      }
    }
  });
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ServicesToRestart {
  web: bool,
  mariadb: bool,
}

fn services_to_restart(status: &AgentStatus) -> ServicesToRestart {
  ServicesToRestart {
    web: status_has_running_services(status),
    mariadb: matches!(
      status.mariadb,
      ServiceState::Starting | ServiceState::Running
    ),
  }
}

fn status_has_running_services(status: &AgentStatus) -> bool {
  [&status.dns, &status.nginx, &status.php_fpm]
    .into_iter()
    .any(|state| matches!(state, ServiceState::Starting | ServiceState::Running))
}

#[cfg(unix)]
fn remove_stale_agent_socket(socket: &Path) -> anyhow::Result<()> {
  let metadata = match std::fs::symlink_metadata(socket) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(error.into()),
  };

  use std::os::unix::fs::FileTypeExt;

  if !metadata.file_type().is_socket() {
    bail!(
      "refusing to remove non-socket Agent path: {}",
      socket.display()
    );
  }
  std::fs::remove_file(socket)
    .with_context(|| format!("unable to remove stale Agent socket: {}", socket.display()))
}

fn resolve_agent_executable() -> anyhow::Result<PathBuf> {
  if let Some(override_path) = std::env::var_os("FABDEV_AGENT_PATH") {
    let override_path = PathBuf::from(override_path);
    if override_path.is_file() {
      return Ok(override_path);
    }
    bail!(
      "FABDEV_AGENT_PATH does not point to a file: {}",
      override_path.display()
    );
  }

  let current_executable = std::env::current_exe().context("unable to locate fabDev Desktop")?;
  resolve_agent_executable_from(&current_executable)
}

fn resolve_agent_executable_from(current_executable: &Path) -> anyhow::Result<PathBuf> {
  let executable_dir = current_executable
    .parent()
    .context("fabDev Desktop executable has no parent directory")?;
  let executable_name = if cfg!(windows) {
    "fabdev-agent.exe"
  } else {
    "fabdev-agent"
  };
  let candidates = [
    executable_dir.join(executable_name),
    executable_dir.join("../Resources").join(executable_name),
    executable_dir
      .join("../Resources/binaries")
      .join(executable_name),
  ];
  candidates
    .into_iter()
    .find(|candidate| candidate.is_file())
    .with_context(|| {
      format!(
        "fabDev Agent is not bundled next to Desktop: {}",
        current_executable.display()
      )
    })
}

fn spawn_agent(executable: &Path, paths: &AppPaths) -> anyhow::Result<()> {
  let log_path = paths.logs.join("agent-process.log");
  let stdout = OpenOptions::new()
    .create(true)
    .append(true)
    .open(&log_path)
    .with_context(|| format!("unable to open Agent log: {}", log_path.display()))?;
  let stderr = stdout
    .try_clone()
    .context("unable to clone Agent log file")?;
  let mut command = Command::new(executable);
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;

    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
  }
  command
    .arg("--data-dir")
    .arg(&paths.root)
    .stdin(Stdio::null())
    .stdout(Stdio::from(stdout))
    .stderr(Stdio::from(stderr));
  command
    .spawn()
    .with_context(|| format!("unable to start fabDev Agent: {}", executable.display()))?;
  Ok(())
}

#[cfg(target_os = "macos")]
fn install_bundled_macos_runtimes(app: &tauri::App) -> anyhow::Result<()> {
  let paths = AppPaths::discover()
    .ok_or_else(|| anyhow::anyhow!("unable to locate fabDev application data"))?;
  paths.ensure()?;
  let source_root = bundled_macos_runtime_root(app)?;

  for spec in BUNDLED_MACOS_RUNTIMES {
    if !should_install_bundled_runtime(&paths, spec.name, spec.version)? {
      continue;
    }
    let stem = format!("{}-{}", spec.name, spec.version);
    let descriptor_path = source_root.join(format!("{stem}.json"));
    let artifact_path = source_root.join(format!("{stem}.tar.gz"));
    let release: RuntimeRelease =
      serde_json::from_reader(std::fs::File::open(&descriptor_path).with_context(|| {
        format!(
          "unable to open built-in Runtime descriptor: {}",
          descriptor_path.display()
        )
      })?)
      .context("invalid built-in Runtime descriptor")?;
    validate_bundled_macos_release(&release, spec, &artifact_path)?;
    install_tar_gz_with_activation(
      &artifact_path,
      &release.sha256,
      spec.name,
      spec.version,
      &paths.runtimes,
      false,
    )?;
    if spec.name == "php" {
      initialize_empty_php_ini_for_runtime(&paths, spec.version)?;
    }
  }

  for spec in BUNDLED_MACOS_RUNTIMES
    .into_iter()
    .filter(|spec| spec.name != "php")
  {
    if active_version(&paths.runtimes, spec.name)?.is_none() {
      set_active_version(&paths.runtimes, spec.name, spec.version)?;
    }
  }
  if active_version(&paths.runtimes, "php")?.is_none() {
    let installed_versions = list_installed_versions(&paths.runtimes, "php")?;
    let default_version = installed_versions
      .iter()
      .find(|version| version.starts_with("8.2."))
      .cloned()
      .or_else(|| installed_versions.into_iter().next())
      .context("no bundled PHP Runtime is installed")?;
    set_active_version(&paths.runtimes, "php", &default_version)?;
  }
  let demo_source = bundled_macos_demo_root(app)?;
  install_bundled_demo(&demo_source, &paths)?;
  Ok(())
}

#[cfg(target_os = "macos")]
fn bundled_macos_runtime_root(app: &tauri::App) -> anyhow::Result<PathBuf> {
  let bundled = app
    .path()
    .resource_dir()
    .context("unable to locate bundled macOS Runtime resources")?
    .join("runtime");
  if bundled_macos_runtime_root_is_complete(&bundled) {
    return Ok(bundled);
  }

  #[cfg(debug_assertions)]
  {
    let development = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/macos");
    if bundled_macos_runtime_root_is_complete(&development) {
      return Ok(development);
    }
  }

  bail!(
    "built-in macOS Runtime resources are missing: {}",
    bundled.display()
  )
}

#[cfg(target_os = "macos")]
fn bundled_macos_runtime_root_is_complete(root: &Path) -> bool {
  root.is_dir()
    && BUNDLED_MACOS_RUNTIMES.into_iter().all(|spec| {
      let stem = format!("{}-{}", spec.name, spec.version);
      root.join(format!("{stem}.json")).is_file() && root.join(format!("{stem}.tar.gz")).is_file()
    })
}

#[cfg(target_os = "macos")]
fn bundled_macos_demo_root(app: &tauri::App) -> anyhow::Result<PathBuf> {
  let bundled = app
    .path()
    .resource_dir()
    .context("unable to locate bundled macOS demo resources")?
    .join("demo");
  if bundled.join("public/index.php").is_file() {
    return Ok(bundled);
  }

  #[cfg(debug_assertions)]
  {
    let development =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../distribution/macos/community/demo");
    if development.join("public/index.php").is_file() {
      return Ok(development);
    }
  }

  bail!("built-in macOS demo resources are missing")
}

#[cfg(target_os = "macos")]
fn validate_bundled_macos_release(
  release: &RuntimeRelease,
  spec: BundledRuntimeSpec,
  artifact_path: &Path,
) -> anyhow::Result<()> {
  if release.name != spec.name || release.version != spec.version {
    bail!(
      "built-in Runtime descriptor does not match {} {}",
      spec.name,
      spec.version
    );
  }
  if release.platform != "macos" || release.architecture != "arm64" {
    bail!("built-in Runtime is not a macOS ARM64 package");
  }
  let expected_archive = format!("{}-{}.tar.gz", spec.name, spec.version);
  if release.url != expected_archive {
    bail!("built-in Runtime descriptor points to an unexpected archive");
  }
  let actual_size = std::fs::metadata(artifact_path)
    .with_context(|| {
      format!(
        "unable to inspect built-in Runtime archive: {}",
        artifact_path.display()
      )
    })?
    .len();
  if actual_size != release.size {
    bail!("built-in Runtime archive size does not match its descriptor");
  }
  Ok(())
}

#[cfg(windows)]
fn install_bundled_windows_runtimes(app: &tauri::App) -> anyhow::Result<()> {
  let paths = AppPaths::discover()
    .ok_or_else(|| anyhow::anyhow!("unable to locate fabDev application data"))?;
  paths.ensure()?;
  let source_root = app
    .path()
    .resource_dir()
    .context("unable to locate bundled Windows Runtime resources")?
    .join("runtime");
  if !source_root.is_dir() {
    bail!(
      "bundled Windows Runtime resources are missing: {}",
      source_root.display()
    );
  }

  install_bundled_directory(
    &source_root.join("nginx/current"),
    &paths.runtimes.join("nginx/current"),
  )?;
  let bundled_php = source_root.join("php");
  for entry in std::fs::read_dir(&bundled_php).with_context(|| {
    format!(
      "unable to list bundled PHP Runtimes: {}",
      bundled_php.display()
    )
  })? {
    let entry = entry?;
    let version = entry.file_name().to_string_lossy().into_owned();
    if entry.file_type()?.is_dir() && should_install_bundled_runtime(&paths, "php", &version)? {
      install_bundled_directory(&entry.path(), &paths.runtimes.join("php").join(&version))?;
      initialize_empty_php_ini_for_runtime(&paths, &version)?;
    }
  }

  if active_version(&paths.runtimes, "php")?.is_none() {
    let installed_versions = list_installed_versions(&paths.runtimes, "php")?;
    let default_version = installed_versions
      .iter()
      .find(|version| version.starts_with("8.2."))
      .cloned()
      .or_else(|| installed_versions.into_iter().next())
      .context("bundled PHP 8.2 Runtime is missing")?;
    set_active_version(&paths.runtimes, "php", &default_version)?;
  }
  install_bundled_demo(&source_root.join("demo"), &paths)?;
  Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn should_install_bundled_runtime(
  paths: &AppPaths,
  name: &str,
  version: &str,
) -> anyhow::Result<bool> {
  Ok(
    !paths.runtimes.join(name).join(version).is_dir()
      && !is_runtime_marked_removed(&paths.runtimes, name, version)?,
  )
}

#[cfg(any(target_os = "macos", windows))]
fn install_bundled_demo(source: &Path, paths: &AppPaths) -> anyhow::Result<()> {
  let repository = SiteRepository::open(paths.database())?;
  if !repository.list()?.is_empty() {
    return Ok(());
  }
  let project = paths.root.join("demo");
  install_bundled_directory(source, &project)?;
  let site = create_site(SiteInput {
    name: Some("fabDev Demo".to_owned()),
    domain: Some("demo.test".to_owned()),
    project_path: project,
    document_root: Some(PathBuf::from("public")),
    php_version: Some(PhpVersion { major: 8, minor: 2 }),
  })?;
  repository.insert(&site)?;
  Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn install_bundled_directory(source: &Path, destination: &Path) -> anyhow::Result<()> {
  if destination.is_dir() {
    return Ok(());
  }
  if !source.is_dir() {
    bail!("bundled Runtime directory is missing: {}", source.display());
  }
  let parent = destination
    .parent()
    .context("Runtime destination has no parent directory")?;
  std::fs::create_dir_all(parent)?;
  let name = destination
    .file_name()
    .context("Runtime destination has no directory name")?
    .to_string_lossy();
  let staging = parent.join(format!(".{name}.installing"));
  if staging.exists() {
    std::fs::remove_dir_all(&staging)?;
  }
  copy_directory_tree(source, &staging)?;
  std::fs::rename(&staging, destination)?;
  Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn copy_directory_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
  std::fs::create_dir_all(destination)?;
  for entry in std::fs::read_dir(source)? {
    let entry = entry?;
    let target = destination.join(entry.file_name());
    if entry.file_type()?.is_dir() {
      copy_directory_tree(&entry.path(), &target)?;
    } else {
      std::fs::copy(entry.path(), target)?;
    }
  }
  Ok(())
}

async fn send_request(
  endpoint: &AgentEndpoint,
  request: AgentRequest,
) -> anyhow::Result<AgentResponse> {
  let response_timeout = match &request {
    AgentRequest::InstallPhpRuntime { .. }
    | AgentRequest::InstallNodeRuntime { .. }
    | AgentRequest::InstallMariaDbRuntime { .. } => AGENT_INSTALL_RESPONSE_TIMEOUT,
    _ => AGENT_RESPONSE_TIMEOUT,
  };
  send_request_with_timeout(endpoint, request, response_timeout).await
}

async fn send_request_with_timeout(
  endpoint: &AgentEndpoint,
  request: AgentRequest,
  response_timeout: Duration,
) -> anyhow::Result<AgentResponse> {
  #[cfg(unix)]
  let stream = {
    let AgentEndpoint::UnixSocket(socket) = endpoint;
    UnixStream::connect(socket).await?
  };
  #[cfg(windows)]
  let stream = {
    let AgentEndpoint::NamedPipe(pipe_name) = endpoint;
    connect_named_pipe(pipe_name).await?
  };
  let (reader, mut writer) = tokio::io::split(stream);
  writer
    .write_all(serde_json::to_string(&request)?.as_bytes())
    .await?;
  writer.write_all(b"\n").await?;
  let mut lines = BufReader::new(reader).lines();
  let line = tokio::time::timeout(response_timeout, lines.next_line())
    .await
    .context("fabDev Agent response timed out")??
    .ok_or_else(|| anyhow::anyhow!("fabDev Agent closed the connection"))?;
  Ok(serde_json::from_str(&line)?)
}

#[cfg(windows)]
async fn connect_named_pipe(pipe_name: &str) -> anyhow::Result<NamedPipeClient> {
  let started = tokio::time::Instant::now();
  loop {
    match ClientOptions::new().open(pipe_name) {
      Ok(client) => return Ok(client),
      Err(error)
        if error.raw_os_error() == Some(231) && started.elapsed() < AGENT_START_TIMEOUT =>
      {
        tokio::time::sleep(AGENT_START_POLL_INTERVAL).await;
      }
      Err(error) => return Err(error.into()),
    }
  }
}

#[cfg(target_os = "macos")]
fn set_console_activation_policy(app: &AppHandle, visible: bool) {
  let policy = if visible {
    tauri::ActivationPolicy::Regular
  } else {
    tauri::ActivationPolicy::Accessory
  };
  if let Err(error) = app.set_activation_policy(policy) {
    eprintln!("unable to update fabDev activation policy: {error}");
  }
}

#[cfg(target_os = "macos")]
fn set_macos_app_icon(app: &AppHandle) {
  use objc2::{AllocAnyThread, MainThreadMarker};
  use objc2_app_kit::{NSApplication, NSImage};
  use objc2_foundation::NSData;

  if let Err(error) = app.run_on_main_thread(|| {
    let Some(main_thread) = MainThreadMarker::new() else {
      eprintln!("unable to set fabDev icon outside the main thread");
      return;
    };
    let data = NSData::with_bytes(MACOS_APP_ICON_PNG);
    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) else {
      eprintln!("unable to decode fabDev icon");
      return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    unsafe { application.setApplicationIconImage(Some(&icon)) };
  }) {
    eprintln!("unable to schedule fabDev icon update: {error}");
  }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_app_icon(_app: &AppHandle) {}

#[cfg(not(target_os = "macos"))]
fn set_console_activation_policy(_app: &AppHandle, _visible: bool) {}

fn show_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
    if !window.is_visible().unwrap_or(false) {
      set_console_activation_policy(app, true);
    }
    set_macos_app_icon(app);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
  }
}

fn tray_service_state(status: &AgentStatus) -> TrayServiceState {
  if status.dns == ServiceState::Running
    && status.nginx == ServiceState::Running
    && status.php_fpm == ServiceState::Running
  {
    TrayServiceState::Running
  } else if status.dns == ServiceState::Stopped
    && status.nginx == ServiceState::Stopped
    && status.php_fpm == ServiceState::Stopped
  {
    TrayServiceState::Stopped
  } else {
    TrayServiceState::Mixed
  }
}

fn tray_mariadb_state(status: &AgentStatus) -> TrayMariaDbState {
  match status.mariadb {
    ServiceState::Running => TrayMariaDbState::Running,
    ServiceState::Installed | ServiceState::Stopped | ServiceState::Failed => {
      TrayMariaDbState::Stopped
    }
    ServiceState::Starting | ServiceState::Stopping | ServiceState::Updating => {
      TrayMariaDbState::Busy
    }
    ServiceState::NotInstalled => TrayMariaDbState::NotInstalled,
  }
}

fn set_tray_service_state(app: &AppHandle, state: TrayServiceState) {
  let Some(items) = app.try_state::<TrayMenuItems>() else {
    return;
  };
  if let Ok(mut service_state) = items.service_state.lock() {
    *service_state = state;
  }
  let _ = items.service_toggle.set_text(tray_toggle_label(state));
  let _ = items.service_toggle.set_enabled(true);
}

fn set_tray_mariadb_state(app: &AppHandle, state: TrayMariaDbState) {
  let Some(items) = app.try_state::<TrayMenuItems>() else {
    return;
  };
  if let Ok(mut mariadb_state) = items.mariadb_state.lock() {
    *mariadb_state = state;
  }
  let _ = items
    .mariadb_toggle
    .set_text(tray_mariadb_toggle_label(state));
  let _ = items.mariadb_toggle.set_enabled(matches!(
    state,
    TrayMariaDbState::Running | TrayMariaDbState::Stopped
  ));
}

fn set_tray_action_busy(app: &AppHandle, target: TrayActionTarget) {
  if let Some(items) = app.try_state::<TrayMenuItems>() {
    match target {
      TrayActionTarget::Web => {
        let _ = items.service_toggle.set_enabled(false);
      }
      TrayActionTarget::MariaDb => {
        let _ = items.mariadb_toggle.set_text("MariaDB Busy…");
        let _ = items.mariadb_toggle.set_enabled(false);
      }
    }
  }
}

fn set_tray_all_busy(app: &AppHandle) {
  if let Some(items) = app.try_state::<TrayMenuItems>() {
    let _ = items.service_toggle.set_enabled(false);
    let _ = items.mariadb_toggle.set_text("MariaDB Busy…");
    let _ = items.mariadb_toggle.set_enabled(false);
  }
}

fn tray_toggle_label(state: TrayServiceState) -> &'static str {
  match state {
    TrayServiceState::Running => "Stop All",
    TrayServiceState::Stopped | TrayServiceState::Mixed => "Start All",
  }
}

fn tray_mariadb_toggle_label(state: TrayMariaDbState) -> &'static str {
  match state {
    TrayMariaDbState::Running => "Stop MariaDB",
    TrayMariaDbState::Stopped => "Start MariaDB",
    TrayMariaDbState::Busy => "MariaDB Busy…",
    TrayMariaDbState::NotInstalled => "MariaDB Not Installed",
  }
}

fn tray_toggle_request(app: &AppHandle) -> AgentRequest {
  let state = app
    .try_state::<TrayMenuItems>()
    .and_then(|items| items.service_state.lock().ok().map(|state| *state))
    .unwrap_or(TrayServiceState::Mixed);
  match state {
    TrayServiceState::Running => AgentRequest::StopAll,
    TrayServiceState::Stopped | TrayServiceState::Mixed => AgentRequest::StartAll,
  }
}

fn tray_mariadb_toggle_request(app: &AppHandle) -> AgentRequest {
  let state = app
    .try_state::<TrayMenuItems>()
    .and_then(|items| items.mariadb_state.lock().ok().map(|state| *state))
    .unwrap_or(TrayMariaDbState::NotInstalled);
  mariadb_toggle_request(state)
}

fn mariadb_toggle_request(state: TrayMariaDbState) -> AgentRequest {
  match state {
    TrayMariaDbState::Running => AgentRequest::StopMariaDb,
    TrayMariaDbState::Stopped | TrayMariaDbState::Busy | TrayMariaDbState::NotInstalled => {
      AgentRequest::StartMariaDb
    }
  }
}

fn update_tray_from_response(app: &AppHandle, response: &AgentResponse) {
  match response {
    AgentResponse::Status(status) => {
      set_tray_service_state(app, tray_service_state(status));
      set_tray_mariadb_state(app, tray_mariadb_state(status));
    }
    AgentResponse::Started => set_tray_service_state(app, TrayServiceState::Running),
    AgentResponse::Stopped => set_tray_service_state(app, TrayServiceState::Stopped),
    AgentResponse::MariaDbStarted => {
      set_tray_mariadb_state(app, TrayMariaDbState::Running);
    }
    AgentResponse::MariaDbStopped => {
      set_tray_mariadb_state(app, TrayMariaDbState::Stopped);
    }
    _ => {}
  }
}

async fn refresh_tray_service_state(app: &AppHandle) {
  match request_agent(AgentRequest::GetStatus).await {
    Ok(response) => update_tray_from_response(app, &response),
    Err(_) => {
      set_tray_service_state(app, TrayServiceState::Mixed);
      set_tray_mariadb_state(app, TrayMariaDbState::NotInstalled);
    }
  }
}

fn run_agent_action(app: AppHandle, request: AgentRequest, target: TrayActionTarget) {
  set_tray_action_busy(&app, target);
  tauri::async_runtime::spawn(async move {
    match request_agent_with_ingress_repair(request).await {
      Ok(AgentResponse::Error { message, .. }) => {
        let _ = app.emit(AGENT_ERROR_EVENT, message);
        refresh_tray_service_state(&app).await;
      }
      Ok(response) => {
        update_tray_from_response(&app, &response);
        let _ = app.emit(SERVICE_STATE_CHANGED_EVENT, ());
      }
      Err(error) => {
        let _ = app.emit(AGENT_ERROR_EVENT, error.to_string());
        refresh_tray_service_state(&app).await;
      }
    }
  });
}

fn handle_tray_action(app: &AppHandle, id: &str) {
  match TrayAction::from_id(id) {
    Some(TrayAction::Open) => show_main_window(app),
    Some(TrayAction::ToggleAll) => {
      run_agent_action(app.clone(), tray_toggle_request(app), TrayActionTarget::Web)
    }
    Some(TrayAction::ToggleMariaDb) => run_agent_action(
      app.clone(),
      tray_mariadb_toggle_request(app),
      TrayActionTarget::MariaDb,
    ),
    Some(TrayAction::Quit) => request_app_quit(app.clone()),
    None => {}
  }
}

#[cfg(target_os = "macos")]
fn build_macos_app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
  let menu = Menu::default(app)?;
  let menu_items = menu.items()?;
  let application_menu = menu_items
    .first()
    .and_then(|item| item.as_submenu())
    .ok_or_else(|| std::io::Error::other("macOS application menu is unavailable"))?;
  let application_items = application_menu.items()?;
  let quit_item = application_items
    .last()
    .filter(|item| item.as_predefined_menuitem().is_some())
    .ok_or_else(|| std::io::Error::other("macOS application Quit item is unavailable"))?;

  application_menu.remove(quit_item)?;
  application_menu.append(&MenuItem::with_id(
    app,
    "quit-fabdev-app",
    "Quit fabDev",
    true,
    Some("CmdOrCtrl+Q"),
  )?)?;
  Ok(menu)
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
  let open = MenuItem::with_id(app, "open-fabdev", "Open fabDev", true, None::<&str>)?;
  let service_toggle = MenuItem::with_id(app, "toggle-all", "Start All", true, None::<&str>)?;
  let mariadb_toggle = MenuItem::with_id(
    app,
    "toggle-mariadb",
    "MariaDB Not Installed",
    false,
    None::<&str>,
  )?;
  let separator = PredefinedMenuItem::separator(app)?;
  let quit = MenuItem::with_id(app, "quit-fabdev", "Quit fabDev", true, None::<&str>)?;
  let menu = Menu::with_items(
    app,
    &[&open, &service_toggle, &mariadb_toggle, &separator, &quit],
  )?;
  let icon = Image::from_bytes(TRAY_ICON_PNG)?;

  app.manage(TrayMenuItems {
    service_toggle: service_toggle.clone(),
    service_state: std::sync::Mutex::new(TrayServiceState::Mixed),
    mariadb_toggle: mariadb_toggle.clone(),
    mariadb_state: std::sync::Mutex::new(TrayMariaDbState::NotInstalled),
  });

  TrayIconBuilder::with_id("fabdev")
    .icon(icon)
    .icon_as_template(cfg!(target_os = "macos"))
    .tooltip("fabDev")
    .menu(&menu)
    .show_menu_on_left_click(true)
    .on_menu_event(|app, event| handle_tray_action(app, event.id().as_ref()))
    .build(app)?;

  Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
  #[cfg(target_os = "macos")]
  let builder = builder.menu(build_macos_app_menu);

  let app = builder
    .setup(|app| {
      #[cfg(target_os = "macos")]
      install_bundled_macos_runtimes(app)?;
      #[cfg(windows)]
      install_bundled_windows_runtimes(app)?;
      setup_tray(app)?;
      let app_handle = app.handle().clone();
      tauri::async_runtime::spawn(async move {
        refresh_tray_service_state(&app_handle).await;
      });
      Ok(())
    })
    .on_window_event(|window, event| {
      if window.label() == "main" {
        if let WindowEvent::CloseRequested { api, .. } = event {
          api.prevent_close();
          if window.hide().is_ok() {
            set_console_activation_policy(window.app_handle(), false);
          }
        }
      }
    })
    .on_menu_event(|app, event| handle_tray_action(app, event.id().as_ref()))
    .invoke_handler(tauri::generate_handler![
      agent_request,
      read_config_transfer_file,
      write_config_transfer_file,
      open_site,
      open_proxy_in_chrome,
      reveal_php_ini,
      reveal_default_php_ini,
      trust_local_ca
    ])
    .build(tauri::generate_context!())
    .expect("error while building fabDev");

  app.run(|app, event| match event {
    tauri::RunEvent::ExitRequested { api, .. } if !EXIT_ALLOWED.load(Ordering::SeqCst) => {
      api.prevent_exit();
      request_app_quit(app.clone());
    }
    #[cfg(target_os = "macos")]
    tauri::RunEvent::Ready => set_macos_app_icon(app),
    #[cfg(target_os = "macos")]
    tauri::RunEvent::Reopen { .. } => {
      show_main_window(app);
    }
    _ => {}
  });
}

#[cfg(test)]
mod tests {
  #[cfg(any(target_os = "macos", windows))]
  use super::initialize_empty_php_ini_for_runtime;
  #[cfg(target_os = "macos")]
  use super::{
    bundled_macos_runtime_root_is_complete, install_bundled_demo, validate_bundled_macos_release,
    BundledRuntimeSpec, BUNDLED_MACOS_RUNTIMES,
  };
  use super::{
    default_php_ini_path, is_system_ingress_error, mariadb_toggle_request, php_ini_path, proxy_url,
    read_config_transfer_file, resolve_agent_executable_from, services_to_restart, site_url,
    status_has_running_services, tray_mariadb_state, tray_mariadb_toggle_label, tray_service_state,
    tray_toggle_label, write_config_transfer_file,
  };
  #[cfg(unix)]
  use super::{remove_stale_agent_socket, send_request_with_timeout, shutdown_agent_at};
  use super::{ServicesToRestart, TrayAction, TrayMariaDbState, TrayServiceState};
  use fabdev_core::{AgentRequest, AgentStatus, AppPaths, PhpVersion, ServiceState};
  use std::time::Duration;

  #[test]
  fn maps_known_tray_menu_ids() {
    assert_eq!(TrayAction::from_id("open-fabdev"), Some(TrayAction::Open));
    assert_eq!(
      TrayAction::from_id("toggle-all"),
      Some(TrayAction::ToggleAll)
    );
    assert_eq!(
      TrayAction::from_id("toggle-mariadb"),
      Some(TrayAction::ToggleMariaDb)
    );
    assert_eq!(TrayAction::from_id("quit-fabdev"), Some(TrayAction::Quit));
    assert_eq!(
      TrayAction::from_id("quit-fabdev-app"),
      Some(TrayAction::Quit)
    );
    assert_eq!(TrayAction::from_id("unknown"), None);
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn defines_and_validates_the_four_built_in_macos_runtimes() {
    assert_eq!(
      BUNDLED_MACOS_RUNTIMES,
      [
        BundledRuntimeSpec {
          name: "dnsmasq",
          version: "2.93",
        },
        BundledRuntimeSpec {
          name: "nginx",
          version: "1.30.4",
        },
        BundledRuntimeSpec {
          name: "php",
          version: "7.4.33",
        },
        BundledRuntimeSpec {
          name: "php",
          version: "8.2.33",
        },
      ]
    );

    let root = std::env::temp_dir().join(format!("fabdev-bundled-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create built-in Runtime fixture");
    assert!(!bundled_macos_runtime_root_is_complete(&root));
    for spec in BUNDLED_MACOS_RUNTIMES {
      let stem = format!("{}-{}", spec.name, spec.version);
      std::fs::write(root.join(format!("{stem}.json")), "fixture")
        .expect("write built-in Runtime descriptor fixture");
      std::fs::write(root.join(format!("{stem}.tar.gz")), "fixture")
        .expect("write built-in Runtime archive fixture");
    }
    assert!(bundled_macos_runtime_root_is_complete(&root));
    let artifact = root.join("php-8.2.33.tar.gz");
    std::fs::write(&artifact, "runtime").expect("write built-in Runtime fixture");
    let release = fabdev_runtime::RuntimeRelease {
      name: "php".to_owned(),
      version: "8.2.33".to_owned(),
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      url: "php-8.2.33.tar.gz".to_owned(),
      size: 7,
      sha256: "fixture".to_owned(),
      signature: "development-ad-hoc".to_owned(),
    };
    validate_bundled_macos_release(
      &release,
      BundledRuntimeSpec {
        name: "php",
        version: "8.2.33",
      },
      &artifact,
    )
    .expect("accept matching built-in Runtime descriptor");
    std::fs::remove_dir_all(root).expect("remove built-in Runtime fixture");
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn does_not_reinstall_an_explicitly_removed_bundled_php_runtime() {
    let root = std::env::temp_dir().join(format!("fabdev-bundled-skip-{}", uuid::Uuid::new_v4()));
    let paths = AppPaths::from_root(root.clone());
    assert!(
      super::should_install_bundled_runtime(&paths, "php", "7.4.33")
        .expect("check missing bundled PHP")
    );

    fabdev_runtime::mark_runtime_removed(&paths.runtimes, "php", "7.4.33")
      .expect("mark bundled PHP removed");
    assert!(
      !super::should_install_bundled_runtime(&paths, "php", "7.4.33")
        .expect("check removed bundled PHP")
    );
    std::fs::remove_dir_all(root).expect("remove bundled skip fixture");
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn creates_demo_only_for_an_empty_site_registry() {
    let root = std::env::temp_dir().join(format!("fabdev-demo-{}", uuid::Uuid::new_v4()));
    let source = root.join("source");
    std::fs::create_dir_all(source.join("public")).expect("create demo source fixture");
    std::fs::write(source.join("public/index.php"), "<?php echo 'fabDev';")
      .expect("write demo source fixture");

    let empty_paths = AppPaths::from_root(root.join("empty-data"));
    empty_paths.ensure().expect("create empty App paths");
    install_bundled_demo(&source, &empty_paths).expect("install default demo");
    let empty_repository = fabdev_core::SiteRepository::open(empty_paths.database())
      .expect("open empty Site repository");
    let sites = empty_repository.list().expect("list default demo");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].domain, "demo.test");
    assert_eq!(
      sites[0].php_version,
      Some(PhpVersion { major: 8, minor: 2 })
    );
    assert!(empty_paths.root.join("demo/public/index.php").is_file());
    install_bundled_demo(&source, &empty_paths).expect("keep the single default demo");
    assert_eq!(
      empty_repository
        .list()
        .expect("list unchanged default demo")
        .len(),
      1
    );

    let existing_paths = AppPaths::from_root(root.join("existing-data"));
    existing_paths.ensure().expect("create existing App paths");
    let existing_repository = fabdev_core::SiteRepository::open(existing_paths.database())
      .expect("open existing Site repository");
    let project = root.join("existing-project");
    std::fs::create_dir_all(&project).expect("create existing project fixture");
    let existing_site = fabdev_core::create_site(fabdev_core::SiteInput {
      name: Some("Existing".to_owned()),
      domain: Some("existing.test".to_owned()),
      project_path: project,
      document_root: None,
      php_version: None,
    })
    .expect("create existing Site fixture");
    existing_repository
      .insert(&existing_site)
      .expect("insert existing Site fixture");
    install_bundled_demo(&source, &existing_paths).expect("preserve existing Site registry");
    let sites = existing_repository
      .list()
      .expect("list existing Site registry");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].domain, "existing.test");
    assert!(!existing_paths.root.join("demo").exists());

    std::fs::remove_dir_all(root).expect("remove demo fixture");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn times_out_when_agent_does_not_respond() {
    let root = std::env::temp_dir().join(format!("fdt-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create Agent timeout fixture directory");
    let socket = root.join("agent.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind Agent fixture socket");
    let server = tokio::spawn(async move {
      let (_stream, _) = listener.accept().await.expect("accept Desktop request");
      tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let error = send_request_with_timeout(
      &fabdev_core::AgentEndpoint::UnixSocket(socket.clone()),
      AgentRequest::Ping,
      Duration::from_millis(20),
    )
    .await
    .expect_err("time out stalled Agent response");

    assert!(error.to_string().contains("response timed out"));
    server.abort();
    let _ = server.await;
    std::fs::remove_dir_all(root).expect("remove Agent timeout fixture directory");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn shuts_down_existing_agent_before_desktop_exit() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let root = std::env::temp_dir().join(format!("fdq-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create Agent fixture directory");
    let socket = root.join("agent.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind Agent fixture socket");
    let server = tokio::spawn(async move {
      for expected_request in ["ping", "stopAll", "shutdown"] {
        let (stream, _) = listener.accept().await.expect("accept Desktop request");
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
          .next_line()
          .await
          .expect("read Agent request")
          .expect("Agent request line");
        let request =
          serde_json::from_str::<fabdev_core::AgentRequest>(&line).expect("parse Agent request");
        let response = match request {
          fabdev_core::AgentRequest::Ping => {
            assert_eq!(expected_request, "ping");
            fabdev_core::AgentResponse::Pong {
              protocol_version: fabdev_core::PROTOCOL_VERSION,
            }
          }
          fabdev_core::AgentRequest::StopAll => {
            assert_eq!(expected_request, "stopAll");
            fabdev_core::AgentResponse::Stopped
          }
          fabdev_core::AgentRequest::Shutdown => {
            assert_eq!(expected_request, "shutdown");
            fabdev_core::AgentResponse::Stopped
          }
          _ => unreachable!(),
        };
        writer
          .write_all(
            serde_json::to_string(&response)
              .expect("serialize response")
              .as_bytes(),
          )
          .await
          .expect("write Agent response");
        writer.write_all(b"\n").await.expect("finish response");
      }
    });

    shutdown_agent_at(&fabdev_core::AgentEndpoint::UnixSocket(socket.clone()))
      .await
      .expect("shutdown Agent before Desktop exit");
    server.await.expect("finish Agent fixture");

    assert!(!socket.exists());
    std::fs::remove_dir_all(root).expect("remove Agent fixture directory");
  }

  #[test]
  fn detects_running_services_for_agent_upgrade() {
    let mut status = AgentStatus::development();
    assert!(!status_has_running_services(&status));
    assert_eq!(services_to_restart(&status), ServicesToRestart::default());
    status.nginx = ServiceState::Running;
    status.mariadb = ServiceState::Running;
    assert!(status_has_running_services(&status));
    assert_eq!(
      services_to_restart(&status),
      ServicesToRestart {
        web: true,
        mariadb: true,
      }
    );
  }

  #[test]
  fn maps_agent_status_to_tray_check_state() {
    let mut status = AgentStatus::development();
    status.dns = ServiceState::Running;
    status.nginx = ServiceState::Running;
    status.php_fpm = ServiceState::Running;
    assert_eq!(tray_service_state(&status), TrayServiceState::Running);

    status.dns = ServiceState::Stopped;
    status.nginx = ServiceState::Stopped;
    status.php_fpm = ServiceState::Stopped;
    assert_eq!(tray_service_state(&status), TrayServiceState::Stopped);

    status.php_fpm = ServiceState::Failed;
    assert_eq!(tray_service_state(&status), TrayServiceState::Mixed);
  }

  #[test]
  fn changes_tray_toggle_label_for_service_state() {
    assert_eq!(tray_toggle_label(TrayServiceState::Running), "Stop All");
    assert_eq!(tray_toggle_label(TrayServiceState::Stopped), "Start All");
    assert_eq!(tray_toggle_label(TrayServiceState::Mixed), "Start All");
  }

  #[test]
  fn maps_mariadb_status_to_tray_state_and_label() {
    let mut status = AgentStatus::development();
    assert_eq!(tray_mariadb_state(&status), TrayMariaDbState::NotInstalled);
    assert_eq!(
      tray_mariadb_toggle_label(TrayMariaDbState::NotInstalled),
      "MariaDB Not Installed"
    );

    status.mariadb = ServiceState::Installed;
    assert_eq!(tray_mariadb_state(&status), TrayMariaDbState::Stopped);
    assert_eq!(
      tray_mariadb_toggle_label(TrayMariaDbState::Stopped),
      "Start MariaDB"
    );

    status.mariadb = ServiceState::Running;
    assert_eq!(tray_mariadb_state(&status), TrayMariaDbState::Running);
    assert_eq!(
      tray_mariadb_toggle_label(TrayMariaDbState::Running),
      "Stop MariaDB"
    );

    status.mariadb = ServiceState::Starting;
    assert_eq!(tray_mariadb_state(&status), TrayMariaDbState::Busy);
    assert_eq!(
      tray_mariadb_toggle_label(TrayMariaDbState::Busy),
      "MariaDB Busy…"
    );

    assert!(matches!(
      mariadb_toggle_request(TrayMariaDbState::Running),
      fabdev_core::AgentRequest::StopMariaDb
    ));
    assert!(matches!(
      mariadb_toggle_request(TrayMariaDbState::Stopped),
      fabdev_core::AgentRequest::StartMariaDb
    ));
  }

  #[test]
  fn identifies_only_the_ingress_error_for_automatic_repair() {
    let ingress = fabdev_core::AgentResponse::Error {
      code: "internal_error".to_owned(),
      message: "system ingress is unavailable on DNS port 53, HTTP port 80, or HTTPS port 443"
        .to_owned(),
    };
    let unrelated = fabdev_core::AgentResponse::Error {
      code: "internal_error".to_owned(),
      message: "unable to start Nginx".to_owned(),
    };

    assert!(is_system_ingress_error(&ingress));
    assert!(!is_system_ingress_error(&unrelated));
    assert!(!is_system_ingress_error(
      &fabdev_core::AgentResponse::Started
    ));
  }

  #[test]
  fn resolves_agent_next_to_desktop_executable() {
    let root = std::env::temp_dir().join(format!("fabdev-desktop-{}", uuid::Uuid::new_v4()));
    let executable_dir = root.join("fabDev.app/Contents/MacOS");
    std::fs::create_dir_all(&executable_dir).expect("create executable directory");
    let desktop = executable_dir.join(if cfg!(windows) {
      "fabdev-desktop.exe"
    } else {
      "fabdev-desktop"
    });
    let agent = executable_dir.join(if cfg!(windows) {
      "fabdev-agent.exe"
    } else {
      "fabdev-agent"
    });
    std::fs::write(&desktop, "fixture").expect("write Desktop fixture");
    std::fs::write(&agent, "fixture").expect("write Agent fixture");

    assert_eq!(
      resolve_agent_executable_from(&desktop).expect("resolve bundled Agent"),
      agent
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  #[cfg(unix)]
  fn refuses_to_remove_non_socket_agent_path() {
    let root = std::env::temp_dir().join(format!("fabdev-file-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create file fixture directory");
    let socket = root.join("agent.sock");
    std::fs::write(&socket, "not a socket").expect("write file fixture");

    let error = remove_stale_agent_socket(&socket).expect_err("reject non-socket path");
    assert!(error.to_string().contains("refusing to remove non-socket"));
    assert!(socket.is_file());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn resolves_managed_php_ini_path() {
    let paths = AppPaths::from_root("/tmp/fabDev");
    assert_eq!(
      php_ini_path(&paths, &PhpVersion { major: 8, minor: 2 }),
      std::path::PathBuf::from("/tmp/fabDev/config/php/8.2/php.ini")
    );
    assert_eq!(
      default_php_ini_path(&paths),
      std::path::PathBuf::from("/tmp/fabDev/config/php/default/php.ini")
    );
  }

  #[test]
  #[cfg(any(target_os = "macos", windows))]
  fn initializes_an_empty_php_ini_for_a_new_bundled_runtime() {
    let root = std::env::temp_dir().join(format!("fabdev-empty-ini-{}", uuid::Uuid::new_v4()));
    let paths = AppPaths::from_root(&root);

    initialize_empty_php_ini_for_runtime(&paths, "7.4.33")
      .expect("initialize empty bundled PHP configuration");

    assert_eq!(
      std::fs::read_to_string(php_ini_path(&paths, &PhpVersion { major: 7, minor: 4 }))
        .expect("read bundled PHP configuration"),
      ""
    );
    std::fs::remove_dir_all(root).expect("remove empty bundled PHP configuration fixture");
  }

  #[test]
  fn reads_and_writes_bounded_json_configuration_files() {
    let root = std::env::temp_dir().join(format!("fabdev-transfer-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create transfer fixture");
    let path = root.join("sites.json");
    let contents = "{\"format\":\"fabdev-sites\",\"version\":1}";

    write_config_transfer_file(path.to_string_lossy().into_owned(), contents.to_owned())
      .expect("write transfer fixture");
    assert_eq!(
      read_config_transfer_file(path.to_string_lossy().into_owned())
        .expect("read transfer fixture"),
      contents
    );
    assert!(write_config_transfer_file(
      root.join("sites.txt").to_string_lossy().into_owned(),
      contents.to_owned()
    )
    .is_err());
    assert!(write_config_transfer_file(
      root.join("invalid.json").to_string_lossy().into_owned(),
      "not json".to_owned()
    )
    .is_err());
    std::fs::remove_dir_all(root).expect("remove transfer fixture");
  }

  #[test]
  fn grants_the_main_window_save_dialog_permission() {
    let capability: serde_json::Value =
      serde_json::from_str(include_str!("../capabilities/default.json"))
        .expect("parse Desktop capability fixture");
    let permissions = capability["permissions"]
      .as_array()
      .expect("read Desktop permissions");
    assert!(permissions
      .iter()
      .any(|permission| permission == "dialog:allow-save"));
  }

  #[test]
  fn builds_site_url_only_for_valid_test_domains() {
    assert_eq!(
      site_url("ERP-Demo.test.", false),
      Ok("http://erp-demo.test".to_owned())
    );
    assert_eq!(
      site_url("ERP-Demo.test.", true),
      Ok("https://erp-demo.test".to_owned())
    );
    assert!(site_url("example.com", false).is_err());
    assert!(site_url("demo.test/path", true).is_err());
  }

  #[test]
  fn builds_proxy_url_only_for_valid_test_domains_and_ports() {
    assert_eq!(
      proxy_url("EXAMPLE.test.", 3010),
      Ok("http://example.test:3010/".to_owned())
    );
    assert!(proxy_url("example.com", 3010).is_err());
    assert!(proxy_url("example.test/path", 3010).is_err());
    assert!(proxy_url("example.test", 80).is_err());
  }
}
