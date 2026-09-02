use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use fabdev_core::{
  default_mariadb_system_socket, site::normalize_domain, AgentStatus, AppPaths,
  MariaDbConnectionMode, MariaDbSettings, PhpFpmPoolStatus, PhpVersion, ServiceState, Site,
  PROTOCOL_VERSION,
};
use fabdev_sites::{render_nginx_site, FastCgiEndpoint, NginxSiteConfig, NginxTlsConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

mod tls;

use tls::ensure_site_certificate;
pub use tls::{ensure_local_ca, remove_site_certificate, LocalSiteCertificate};

const NGINX_CONFIG_TEMPLATE: &str = include_str!("../../../resources/nginx/nginx.conf");
const PHP_INI_TEMPLATE: &str = include_str!("../../../resources/php/php.ini");
const PHP_74_INI_TEMPLATE: &str = include_str!("../../../resources/php/php-7.4.ini");
const PHP_82_INI_TEMPLATE: &str = include_str!("../../../resources/php/php-8.2.ini");
const PHP_WINDOWS_INI_TEMPLATE: &str = include_str!("../../../resources/php/php-windows.ini");
const PHP_FPM_TEMPLATE: &str = include_str!("../../../resources/php/php-fpm.conf");
const PHP_POOL_TEMPLATE: &str = include_str!("../../../resources/php/www.conf");
const MARIADB_CONFIG_TEMPLATE: &str = include_str!("../../../resources/mariadb/my.cnf");
const MARIADB_CUSTOM_CONFIG_TEMPLATE: &str = "[mariadbd]\n\n";
const MARIADB_CONFIG_MAX_BYTES: usize = 512 * 1024;
const PHP_FPM_STATUS_PATH: &str = "/__fabdev/php-fpm-status";
const MANAGED_LOG_MAX_BYTES: u64 = 20 * 1024 * 1024;
const MANAGED_LOG_RETENTION: usize = 7;
const LOG_ROTATION_CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct RuntimePaths {
  pub dnsmasq: PathBuf,
  pub nginx: PathBuf,
  pub php: PathBuf,
  pub mariadb: PathBuf,
}

impl RuntimePaths {
  pub fn from_runtime_root(root: impl AsRef<Path>) -> Self {
    let root = root.as_ref();
    Self {
      dnsmasq: root.join("dnsmasq/current"),
      nginx: root.join("nginx/current"),
      php: root.join("php"),
      mariadb: root.join("mariadb/current"),
    }
  }

  pub fn base_services_installed(&self) -> bool {
    nginx_binary(&self.nginx).is_file()
      && (cfg!(windows) || dnsmasq_binary(&self.dnsmasq).is_file())
  }

  pub fn resolve_php(&self, version: &PhpVersion) -> Result<PathBuf> {
    if php_server_binary(&self.php).is_file() {
      return Ok(self.php.clone());
    }

    let prefix = format!("{version}.");
    let entries = std::fs::read_dir(&self.php)
      .with_context(|| format!("PHP {version} Runtime is not installed"))?;
    let mut candidates = entries
      .filter_map(|entry| entry.ok())
      .filter_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        let patch = name.strip_prefix(&prefix)?.parse::<u16>().ok()?;
        let path = entry.path();
        php_server_binary(&path).is_file().then_some((patch, path))
      })
      .collect::<Vec<_>>();
    candidates.sort_by_key(|(patch, _)| *patch);
    candidates
      .pop()
      .map(|(_, path)| path)
      .with_context(|| format!("PHP {version} Runtime is not installed"))
  }

  pub fn has_any_php(&self) -> bool {
    if php_server_binary(&self.php).is_file() {
      return true;
    }
    std::fs::read_dir(&self.php)
      .map(|entries| {
        entries
          .filter_map(|entry| entry.ok())
          .any(|entry| php_server_binary(&entry.path()).is_file())
      })
      .unwrap_or(false)
  }

  fn installed_php_versions(&self) -> BTreeSet<PhpVersion> {
    std::fs::read_dir(&self.php)
      .map(|entries| {
        entries
          .filter_map(|entry| entry.ok())
          .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut parts = name.split('.');
            let major = parts.next()?.parse::<u8>().ok()?;
            let minor = parts.next()?.parse::<u8>().ok()?;
            parts.next()?.parse::<u16>().ok()?;
            if parts.next().is_some() || major < 7 || !php_server_binary(&entry.path()).is_file() {
              return None;
            }
            Some(PhpVersion { major, minor })
          })
          .collect()
      })
      .unwrap_or_default()
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServicePorts {
  pub dns: u16,
  pub http: u16,
  pub https: u16,
  pub mariadb: u16,
}

impl ServicePorts {
  pub const fn system() -> Self {
    Self {
      dns: 53,
      http: 80,
      https: 443,
      mariadb: 3306,
    }
  }
}

#[derive(Clone, Debug)]
pub struct GeneratedPhpConfig {
  pub version: PhpVersion,
  pub runtime: PathBuf,
  pub php_fpm: PathBuf,
  pub php_ini: PathBuf,
  pub php_socket: PathBuf,
  pub fastcgi_endpoint: FastCgiEndpoint,
}

#[derive(Clone, Debug)]
pub struct GeneratedConfigs {
  pub dnsmasq: PathBuf,
  pub nginx: PathBuf,
  pub php: Vec<GeneratedPhpConfig>,
}

#[derive(Clone, Debug)]
pub struct GeneratedMariaDbConfig {
  pub config: PathBuf,
  pub data: PathBuf,
  pub pid: PathBuf,
  pub socket: PathBuf,
}

pub struct ServiceSupervisor {
  paths: AppPaths,
  runtimes: RuntimePaths,
  ports: ServicePorts,
  ingress_ports: Option<ServicePorts>,
  dnsmasq: Option<Child>,
  nginx: Option<Child>,
  php_fpm: HashMap<PhpVersion, Child>,
  expected_php_versions: BTreeSet<PhpVersion>,
  mariadb: Option<Child>,
  recovered_mariadb_pid: Option<u32>,
  last_log_rotation_check: Option<Instant>,
}

impl ServiceSupervisor {
  pub fn new(paths: AppPaths, runtimes: RuntimePaths, ports: ServicePorts) -> Self {
    #[cfg(unix)]
    let ingress_ports = (ports != ServicePorts::system()).then_some(ServicePorts::system());
    #[cfg(windows)]
    let ingress_ports = None;
    let recovered_mariadb_pid = recover_mariadb_pid(&paths, &runtimes);
    Self {
      paths,
      runtimes,
      ports,
      ingress_ports,
      dnsmasq: None,
      nginx: None,
      php_fpm: HashMap::new(),
      expected_php_versions: BTreeSet::new(),
      mariadb: None,
      recovered_mariadb_pid,
      last_log_rotation_check: None,
    }
  }

  pub fn set_mariadb_runtime(&mut self, runtime: PathBuf) {
    self.runtimes.mariadb = runtime;
    self.recovered_mariadb_pid = recover_mariadb_pid(&self.paths, &self.runtimes);
  }

  pub fn status(&mut self) -> AgentStatus {
    #[cfg(unix)]
    let mut dns = child_state(&mut self.dnsmasq, dnsmasq_binary(&self.runtimes.dnsmasq));
    #[cfg(windows)]
    let mut dns = if self.nginx.is_some() {
      ServiceState::Running
    } else {
      ServiceState::Installed
    };
    let mut nginx = child_state(&mut self.nginx, nginx_binary(&self.runtimes.nginx));
    if let Some(ports) = self.ingress_ports {
      if dns == ServiceState::Running && !dns_ingress_ready(ports.dns) {
        dns = ServiceState::Failed;
      }
      if nginx == ServiceState::Running && !http_ingress_ready(ports.http) {
        nginx = ServiceState::Failed;
      }
      if nginx == ServiceState::Running && !http_ingress_ready(ports.https) {
        nginx = ServiceState::Failed;
      }
    }

    AgentStatus {
      protocol_version: PROTOCOL_VERSION,
      agent_version: env!("CARGO_PKG_VERSION").to_owned(),
      dns,
      nginx,
      php_fpm: php_fpm_state(
        &mut self.php_fpm,
        &self.expected_php_versions,
        &self.runtimes,
      ),
      php_fpm_pools: Vec::new(),
      mariadb: mariadb_state(
        &mut self.mariadb,
        &mut self.recovered_mariadb_pid,
        mariadb_server_binary(&self.runtimes.mariadb),
      ),
    }
  }

  pub async fn php_fpm_pool_statuses(&self, sites: &[Site]) -> Vec<PhpFpmPoolStatus> {
    let mut statuses = Vec::new();
    for version in &self.expected_php_versions {
      let Some(site) = sites.iter().find(|site| {
        site.enabled
          && site
            .php_version
            .as_ref()
            .is_some_and(|site_version| site_version == version)
      }) else {
        continue;
      };
      match query_php_fpm_status(self.ports.http, &site.domain, version).await {
        Ok(status) => statuses.push(status),
        Err(error) => eprintln!("unable to read PHP {version} FPM status: {error:#}"),
      }
    }
    statuses
  }

  pub fn rotate_logs_if_due(&mut self) -> Result<()> {
    let now = Instant::now();
    if self
      .last_log_rotation_check
      .is_some_and(|last| now.duration_since(last) < LOG_ROTATION_CHECK_INTERVAL)
    {
      return Ok(());
    }
    self.last_log_rotation_check = Some(now);
    rotate_managed_logs(&self.paths, MANAGED_LOG_MAX_BYTES, MANAGED_LOG_RETENTION)
  }

  pub async fn start_mariadb(&mut self) -> Result<()> {
    if self.mariadb.is_some() || recovered_process_running(&mut self.recovered_mariadb_pid) {
      bail!("fabDev MariaDB is already running");
    }
    let server = mariadb_server_binary(&self.runtimes.mariadb);
    if !server.is_file() {
      bail!("fabDev MariaDB Runtime is not installed");
    }
    let settings = self.mariadb_settings()?;
    ensure_tcp_port_available(settings.port, "MariaDB")?;

    let config =
      generate_mariadb_config_with_settings(&self.paths, &self.runtimes.mariadb, &settings)?;
    if !config.data.join("mysql").is_dir() {
      initialize_mariadb(&self.runtimes.mariadb, &config).await?;
    }
    remove_file_if_exists(&config.pid)?;
    remove_file_if_exists(&config.socket)?;
    self.recovered_mariadb_pid = None;
    self.mariadb = Some(spawn_service(
      server,
      [format!("--defaults-file={}", config.config.display())],
      self.paths.logs.join("mariadb-process.log"),
    )?);
    #[cfg(unix)]
    if let Err(error) = wait_for_path(&config.socket, Duration::from_secs(10)).await {
      let _ = self.stop_mariadb().await;
      return Err(error.context("fabDev MariaDB did not create its Unix Socket"));
    }
    if let Err(error) = wait_for_tcp_port(settings.port, Duration::from_secs(10)).await {
      let _ = self.stop_mariadb().await;
      return Err(error.context("fabDev MariaDB did not become ready"));
    }
    Ok(())
  }

  pub async fn stop_mariadb(&mut self) -> Result<()> {
    let mut errors = Vec::new();
    collect_stop_error(
      &mut errors,
      "MariaDB child process",
      stop_child(&mut self.mariadb).await,
    );
    #[cfg(unix)]
    if let Some(pid) = self.recovered_mariadb_pid.take() {
      collect_stop_error(
        &mut errors,
        "recovered MariaDB process",
        stop_process(pid, "recovered fabDev MariaDB").await,
      );
    }
    #[cfg(not(unix))]
    {
      self.recovered_mariadb_pid = None;
    }
    collect_stop_error(
      &mut errors,
      "untracked MariaDB processes",
      stop_untracked_mariadb_processes(&self.paths, &self.runtimes).await,
    );
    let service = self.paths.services.join("mariadb");
    collect_stop_error(
      &mut errors,
      "MariaDB PID file",
      remove_file_if_exists(&service.join("mariadb.pid")),
    );
    collect_stop_error(
      &mut errors,
      "MariaDB Socket",
      remove_file_if_exists(&service.join("mariadb.sock")),
    );
    finish_stop(errors)
  }

  pub async fn start_mariadb_and_remember(&mut self) -> Result<()> {
    if matches!(self.status().mariadb, ServiceState::Running) {
      save_mariadb_desired_state(&self.paths, true)?;
      return self.refresh_php_mariadb_connection().await;
    }
    let previous_state = load_mariadb_desired_state(&self.paths)?;
    save_mariadb_desired_state(&self.paths, true)?;
    if let Err(error) = self.start_mariadb().await {
      let _ = save_mariadb_desired_state(&self.paths, previous_state);
      return Err(error);
    }
    if let Err(error) = self.refresh_php_mariadb_connection().await {
      let _ = self.stop_mariadb().await;
      let _ = self.refresh_php_mariadb_connection().await;
      let _ = save_mariadb_desired_state(&self.paths, previous_state);
      return Err(error);
    }
    Ok(())
  }

  pub async fn stop_mariadb_and_remember(&mut self) -> Result<()> {
    let previous_state = load_mariadb_desired_state(&self.paths)?;
    save_mariadb_desired_state(&self.paths, false)?;
    if let Err(error) = self.stop_mariadb().await {
      let _ = save_mariadb_desired_state(&self.paths, previous_state);
      return Err(error);
    }
    self.refresh_php_mariadb_connection().await
  }

  pub async fn restore_mariadb_last_state(&mut self) -> Result<()> {
    if !load_mariadb_desired_state(&self.paths)? {
      return Ok(());
    }
    if matches!(self.status().mariadb, ServiceState::Running) {
      return self.refresh_php_mariadb_connection().await;
    }
    if !mariadb_server_binary(&self.runtimes.mariadb).is_file() {
      return Ok(());
    }
    self.start_mariadb().await?;
    self.refresh_php_mariadb_connection().await
  }

  pub fn remember_mariadb_stopped(&self) -> Result<()> {
    save_mariadb_desired_state(&self.paths, false)
  }

  pub fn mariadb_settings(&self) -> Result<MariaDbSettings> {
    load_mariadb_settings(&self.paths, self.ports.mariadb)
  }

  pub fn save_mariadb_settings(&mut self, settings: MariaDbSettings) -> Result<MariaDbSettings> {
    if self.mariadb.is_some() || recovered_process_running(&mut self.recovered_mariadb_pid) {
      bail!("stop MariaDB before changing its settings");
    }
    let settings = validate_mariadb_settings(settings)?;
    save_mariadb_settings_file(&self.paths, &settings)?;
    Ok(settings)
  }

  pub async fn save_mariadb_settings_and_apply(
    &mut self,
    settings: MariaDbSettings,
  ) -> Result<MariaDbSettings> {
    let previous = self.mariadb_settings()?;
    let settings = self.save_mariadb_settings(settings)?;
    if let Err(error) = self.refresh_php_mariadb_connection().await {
      save_mariadb_settings_file(&self.paths, &previous)?;
      let _ = self.refresh_php_mariadb_connection().await;
      return Err(error.context("unable to apply MariaDB connection to PHP-FPM"));
    }
    Ok(settings)
  }

  pub async fn refresh_php_mariadb_connection(&mut self) -> Result<()> {
    let active_versions = self.expected_php_versions.clone();
    let installed_versions = self.runtimes.installed_php_versions();
    for version in installed_versions.difference(&active_versions) {
      generate_php_config(&self.paths, &self.runtimes, version)
        .with_context(|| format!("unable to refresh inactive PHP {version} MariaDB connection"))?;
    }
    if active_versions.is_empty() {
      return Ok(());
    }
    self
      .restart_php_versions(&active_versions)
      .await
      .context("unable to apply the automatic MariaDB connection to PHP-FPM")
  }

  pub fn read_mariadb_config(&self) -> Result<(String, String)> {
    let path = ensure_mariadb_custom_config(&self.paths)?;
    let contents = std::fs::read_to_string(&path)
      .with_context(|| format!("unable to read MariaDB configuration: {}", path.display()))?;
    Ok((mariadb_config_filename().to_owned(), contents))
  }

  pub fn save_mariadb_config(&mut self, contents: &str) -> Result<(String, String)> {
    if self.mariadb.is_some() || recovered_process_running(&mut self.recovered_mariadb_pid) {
      bail!("stop MariaDB before changing its configuration");
    }
    validate_mariadb_custom_config(contents)?;

    let server = mariadb_server_binary(&self.runtimes.mariadb);
    if !server.is_file() {
      bail!("fabDev MariaDB Runtime is not installed");
    }
    let settings = self.mariadb_settings()?;
    let staging = self
      .paths
      .config
      .join("mariadb")
      .join(format!(".{}.validating", mariadb_config_filename()));
    if let Some(parent) = staging.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let rendered = render_mariadb_config(&self.paths, &self.runtimes.mariadb, &settings, contents);
    std::fs::write(&staging, rendered).with_context(|| {
      format!(
        "unable to stage MariaDB configuration: {}",
        staging.display()
      )
    })?;
    let validation = run_check(
      server,
      [
        format!("--defaults-file={}", staging.display()),
        "--verbose".to_owned(),
        "--help".to_owned(),
      ],
      "MariaDB",
    );
    let _ = std::fs::remove_file(&staging);
    validation?;

    save_mariadb_custom_config(&self.paths, contents)?;
    Ok((mariadb_config_filename().to_owned(), contents.to_owned()))
  }

  pub async fn set_mariadb_root_password(
    &mut self,
    current_password: &str,
    new_password: &str,
  ) -> Result<()> {
    if new_password.is_empty() || new_password.len() > 256 || new_password.contains('\0') {
      bail!("MariaDB root password must contain between 1 and 256 bytes");
    }
    if current_password.len() > 256 || current_password.contains('\0') {
      bail!("current MariaDB root password is invalid");
    }

    let child_running = match self.mariadb.as_mut() {
      Some(child) => child.try_wait()?.is_none(),
      None => false,
    };
    if !child_running && !recovered_process_running(&mut self.recovered_mariadb_pid) {
      bail!("start MariaDB before changing the root password");
    }

    let client = mariadb_client_binary(&self.runtimes.mariadb);
    if !client.is_file() {
      bail!(
        "fabDev MariaDB Runtime does not contain the MariaDB client: {}",
        client.display()
      );
    }
    let settings = self.mariadb_settings()?;
    let arguments = vec![
      "--no-defaults".to_owned(),
      "--user=root".to_owned(),
      "--connect-timeout=5".to_owned(),
      "--batch".to_owned(),
      "--skip-column-names".to_owned(),
      "--protocol=tcp".to_owned(),
      "--host=127.0.0.1".to_owned(),
      format!("--port={}", settings.port),
    ];

    let statement = mariadb_password_statement(new_password);
    let mut command = Command::new(&client);
    command
      .args(arguments)
      .stdin(Stdio::piped())
      .stdout(Stdio::null())
      .stderr(Stdio::piped());
    if !current_password.is_empty() {
      command.env("MYSQL_PWD", current_password);
    }
    let mut child = command
      .spawn()
      .with_context(|| format!("unable to run MariaDB client: {}", client.display()))?;
    child
      .stdin
      .take()
      .context("unable to open MariaDB client input")?
      .write_all(statement.as_bytes())
      .await?;
    let output = child.wait_with_output().await?;
    if !output.status.success() {
      bail!(mariadb_password_error(&output.stderr));
    }
    Ok(())
  }

  pub async fn shutdown(&mut self) -> Result<()> {
    let mut errors = Vec::new();
    collect_stop_error(&mut errors, "Web services", self.stop_all().await);
    collect_stop_error(&mut errors, "MariaDB", self.stop_mariadb().await);
    finish_stop(errors)
  }

  pub async fn start_all(&mut self, sites: &[Site]) -> Result<()> {
    if self.dnsmasq.is_some() || self.nginx.is_some() || !self.php_fpm.is_empty() {
      bail!("one or more fabDev services are already running");
    }
    if !self.runtimes.base_services_installed() {
      bail!("required domain routing or Nginx Runtime is not installed");
    }
    rotate_managed_logs(&self.paths, MANAGED_LOG_MAX_BYTES, MANAGED_LOG_RETENTION)?;
    self.last_log_rotation_check = Some(Instant::now());
    self.sync_site_domains(sites).await?;

    let configs = generate_configs(&self.paths, &self.runtimes, self.ports, sites)?;
    validate_configs(&self.runtimes, &configs)?;
    stop_untracked_web_processes(&self.paths, &self.runtimes).await?;
    remove_web_service_artifacts(&self.paths)?;

    if let Err(error) = self.start_validated(&configs).await {
      let _ = self.stop_all().await;
      return Err(error);
    }
    if let Some(ports) = self.ingress_ports {
      if let Err(error) = wait_for_ingress(ports, Duration::from_secs(5)).await {
        let _ = self.stop_all().await;
        return Err(error);
      }
    }
    Ok(())
  }

  pub async fn sync_site_domains(&self, sites: &[Site]) -> Result<()> {
    #[cfg(unix)]
    {
      let _ = sites;
      Ok(())
    }

    #[cfg(windows)]
    {
      let executable = std::env::current_exe().context("unable to locate fabDev Agent")?;
      let helper = executable
        .parent()
        .context("fabDev Agent executable has no parent directory")?
        .join("fabdev-windows-helper.exe");
      if !helper.is_file() {
        bail!("fabDev Windows Helper is not bundled: {}", helper.display());
      }
      let domains = sites
        .iter()
        .filter(|site| site.enabled)
        .map(|site| site.domain.as_str());
      let status = Command::new(&helper)
        .arg("sync-hosts")
        .args(domains)
        .status()
        .await
        .with_context(|| format!("unable to start Windows Helper: {}", helper.display()))?;
      if !status.success() {
        bail!("Windows Helper could not synchronize .test domains");
      }
      Ok(())
    }
  }

  async fn start_validated(&mut self, configs: &GeneratedConfigs) -> Result<()> {
    #[cfg(unix)]
    {
      self.dnsmasq = Some(spawn_service(
        dnsmasq_binary(&self.runtimes.dnsmasq),
        [
          "--keep-in-foreground".to_owned(),
          format!("--conf-file={}", configs.dnsmasq.display()),
        ],
        self.paths.logs.join("dnsmasq-process.log"),
      )?);
    }

    for config in &configs.php {
      let child = spawn_php_fpm(config, &self.paths.logs)?;
      self.php_fpm.insert(config.version.clone(), child);
      self.expected_php_versions.insert(config.version.clone());
    }
    for config in &configs.php {
      wait_for_fastcgi(&config.fastcgi_endpoint, Duration::from_secs(5)).await?;
    }
    self.nginx = Some(spawn_service(
      nginx_binary(&self.runtimes.nginx),
      [
        "-p".to_owned(),
        format!("{}/", self.runtimes.nginx.display()),
        "-c".to_owned(),
        configs.nginx.to_string_lossy().into_owned(),
        "-g".to_owned(),
        "daemon off;".to_owned(),
      ],
      self.paths.logs.join("nginx-process.log"),
    )?);
    Ok(())
  }

  pub async fn stop_all(&mut self) -> Result<()> {
    let mut errors = Vec::new();
    collect_stop_error(
      &mut errors,
      "Nginx child process",
      stop_child(&mut self.nginx).await,
    );
    let php_versions = self
      .php_fpm
      .keys()
      .cloned()
      .chain(self.expected_php_versions.iter().cloned())
      .collect::<BTreeSet<_>>();
    for version in &php_versions {
      let mut child = self.php_fpm.remove(version);
      collect_stop_error(
        &mut errors,
        &format!("PHP {version} child process"),
        stop_child(&mut child).await,
      );
    }
    self.expected_php_versions.clear();
    collect_stop_error(
      &mut errors,
      "dnsmasq child process",
      stop_child(&mut self.dnsmasq).await,
    );
    collect_stop_error(
      &mut errors,
      "untracked Web processes",
      stop_untracked_web_processes(&self.paths, &self.runtimes).await,
    );
    collect_stop_error(
      &mut errors,
      "Web service PID and Socket files",
      remove_web_service_artifacts(&self.paths),
    );
    finish_stop(errors)
  }

  pub async fn remove_site_config(&mut self, site: &Site, remaining_sites: &[Site]) -> Result<()> {
    let domain = normalize_domain(&site.domain).context("invalid Site domain in registry")?;
    if domain != site.domain {
      bail!("Site domain is not normalized: {}", site.domain);
    }

    let config_path = self.paths.sites.join(format!("{domain}.conf"));
    let existing_config = match std::fs::read(&config_path) {
      Ok(config) => Some(config),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Err(error) => return Err(error.into()),
    };
    if existing_config.is_some() {
      std::fs::remove_file(&config_path)
        .with_context(|| format!("unable to remove Site config at {}", config_path.display()))?;
    }

    if existing_config.is_some() && self.nginx_running() {
      let nginx_config = self.paths.services.join("nginx/nginx.conf");
      let reload_result = validate_nginx_config(&self.runtimes, &nginx_config)
        .and_then(|()| reload_nginx(&self.runtimes, &nginx_config));
      if let Err(error) = reload_result {
        restore_site_config(&config_path, existing_config)?;
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        return Err(error.context("unable to apply Site removal to Nginx"));
      }
      if let Err(error) =
        wait_for_default_server(self.ports.http, &site.domain, Duration::from_secs(3)).await
      {
        restore_site_config(&config_path, existing_config)?;
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        return Err(error.context("Nginx did not finish applying Site removal"));
      }
    }

    if let Some(version) = &site.php_version {
      let version_still_used = remaining_sites
        .iter()
        .any(|remaining| remaining.enabled && remaining.php_version.as_ref() == Some(version));
      if !version_still_used {
        let _ = self.stop_php_version(version).await;
      }
    }
    if site.secured {
      remove_site_certificate(&self.paths, &site.domain)?;
    }
    Ok(())
  }

  pub async fn add_site_config(&mut self, site: &Site) -> Result<()> {
    self.paths.ensure()?;
    let php_config = site
      .php_version
      .as_ref()
      .map(|version| generate_php_config(&self.paths, &self.runtimes, version))
      .transpose()?;
    let (config_path, rendered_config) =
      render_site_config(&self.paths, &self.runtimes, self.ports, site)?;
    let existing_config = match std::fs::read(&config_path) {
      Ok(config) => Some(config),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Err(error) => return Err(error.into()),
    };
    std::fs::write(&config_path, rendered_config)
      .with_context(|| format!("unable to write Site config at {}", config_path.display()))?;

    let nginx_running = self.nginx_running();
    if nginx_running {
      let php_started = match &php_config {
        Some(config) => {
          validate_php_config(config)?;
          match self.ensure_php_version_running(config).await {
            Ok(started) => started,
            Err(error) => {
              restore_site_config(&config_path, existing_config)?;
              return Err(error.context("unable to start Site PHP-FPM Runtime"));
            }
          }
        }
        None => false,
      };
      let nginx_config = self.paths.services.join("nginx/nginx.conf");
      let reload_result = validate_nginx_config(&self.runtimes, &nginx_config)
        .and_then(|()| reload_nginx(&self.runtimes, &nginx_config));
      if let Err(error) = reload_result {
        restore_site_config(&config_path, existing_config)?;
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        if php_started {
          if let Some(version) = &site.php_version {
            let _ = self.stop_php_version(version).await;
          }
        }
        return Err(error.context("unable to apply Site addition to Nginx"));
      }
    }
    Ok(())
  }

  pub async fn apply_site_config_batch(
    &mut self,
    previous_sites: &[Site],
    updated_sites: &[Site],
    all_sites_before_update: &[Site],
    all_sites_after_update: &[Site],
  ) -> Result<()> {
    if previous_sites == updated_sites {
      return Ok(());
    }

    self.paths.ensure()?;
    let mut affected_paths = BTreeSet::new();
    for site in previous_sites.iter().chain(updated_sites) {
      let domain = normalize_domain(&site.domain).context("invalid Site domain in registry")?;
      if domain != site.domain {
        bail!("Site domain is not normalized: {}", site.domain);
      }
      affected_paths.insert(self.paths.sites.join(format!("{domain}.conf")));
    }

    let php_versions = updated_sites
      .iter()
      .filter_map(|site| site.php_version.clone())
      .collect::<BTreeSet<_>>();
    let php_configs = php_versions
      .iter()
      .map(|version| generate_php_config(&self.paths, &self.runtimes, version))
      .collect::<Result<Vec<_>>>()?;

    let new_certificate_domains = updated_sites
      .iter()
      .filter(|site| site.secured)
      .filter(|site| {
        let directory = self.paths.config.join("tls/sites");
        !directory.join(format!("{}.crt", site.domain)).is_file()
          || !directory.join(format!("{}.key", site.domain)).is_file()
      })
      .map(|site| site.domain.clone())
      .collect::<BTreeSet<_>>();
    let rendered_configs = match updated_sites
      .iter()
      .map(|site| render_site_config(&self.paths, &self.runtimes, self.ports, site))
      .collect::<Result<Vec<_>>>()
    {
      Ok(configs) => configs,
      Err(error) => {
        remove_new_site_certificates(&self.paths, &new_certificate_domains);
        return Err(error.context("unable to render Site configuration batch"));
      }
    };

    let nginx_running = self.nginx_running();
    let mut started_php_versions = BTreeSet::new();
    if nginx_running {
      for config in &php_configs {
        if let Err(error) = validate_php_config(config) {
          remove_new_site_certificates(&self.paths, &new_certificate_domains);
          return Err(error.context("invalid Site PHP-FPM configuration"));
        }
        match self.ensure_php_version_running(config).await {
          Ok(true) => {
            started_php_versions.insert(config.version.clone());
          }
          Ok(false) => {}
          Err(error) => {
            remove_new_site_certificates(&self.paths, &new_certificate_domains);
            self
              .stop_unused_started_php_versions(&started_php_versions, all_sites_before_update)
              .await;
            return Err(error.context("unable to start Site PHP-FPM Runtime batch"));
          }
        }
      }
    }

    let nginx_config = self.paths.services.join("nginx/nginx.conf");
    let apply_result =
      apply_site_config_files(&affected_paths, &rendered_configs, nginx_running, || {
        validate_nginx_config(&self.runtimes, &nginx_config)
          .and_then(|()| reload_nginx(&self.runtimes, &nginx_config))
      });
    if let Err(error) = apply_result {
      remove_new_site_certificates(&self.paths, &new_certificate_domains);
      self
        .stop_unused_started_php_versions(&started_php_versions, all_sites_before_update)
        .await;
      return Err(error.context("unable to apply Site configuration batch to Nginx"));
    }

    let updated_secured_domains = updated_sites
      .iter()
      .filter(|site| site.secured)
      .map(|site| site.domain.as_str())
      .collect::<BTreeSet<_>>();
    for site in previous_sites
      .iter()
      .filter(|site| site.secured && !updated_secured_domains.contains(site.domain.as_str()))
    {
      if let Err(error) = remove_site_certificate(&self.paths, &site.domain) {
        eprintln!(
          "unable to remove obsolete Site certificate for {}: {error:#}",
          site.domain
        );
      }
    }

    let previous_php_versions = previous_sites
      .iter()
      .filter_map(|site| site.php_version.clone())
      .collect::<BTreeSet<_>>();
    for version in previous_php_versions {
      let version_still_used = all_sites_after_update
        .iter()
        .any(|site| site.enabled && site.php_version.as_ref() == Some(&version));
      if !version_still_used {
        let _ = self.stop_php_version(&version).await;
      }
    }
    Ok(())
  }

  async fn stop_unused_started_php_versions(
    &mut self,
    versions: &BTreeSet<PhpVersion>,
    sites: &[Site],
  ) {
    for version in versions {
      let version_was_used = sites
        .iter()
        .any(|site| site.enabled && site.php_version.as_ref() == Some(version));
      if !version_was_used {
        let _ = self.stop_php_version(version).await;
      }
    }
  }

  pub async fn update_site_config(&mut self, previous: &Site, updated: &Site) -> Result<()> {
    if previous.domain == updated.domain
      && previous.project_path == updated.project_path
      && previous.document_root == updated.document_root
    {
      return Ok(());
    }

    self.paths.ensure()?;
    let previous_domain =
      normalize_domain(&previous.domain).context("invalid previous Site domain in registry")?;
    if previous_domain != previous.domain {
      bail!(
        "previous Site domain is not normalized: {}",
        previous.domain
      );
    }
    let previous_path = self.paths.sites.join(format!("{previous_domain}.conf"));
    let (updated_path, rendered_config) =
      render_site_config(&self.paths, &self.runtimes, self.ports, updated)?;
    let previous_config = match std::fs::read(&previous_path) {
      Ok(config) => Some(config),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Err(error) => return Err(error.into()),
    };
    let updated_config = if updated_path == previous_path {
      previous_config.clone()
    } else {
      match std::fs::read(&updated_path) {
        Ok(config) => Some(config),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
      }
    };

    if let Err(error) = std::fs::write(&updated_path, rendered_config) {
      if previous.secured && previous.domain != updated.domain {
        let _ = remove_site_certificate(&self.paths, &updated.domain);
      }
      return Err(error)
        .with_context(|| format!("unable to write Site config at {}", updated_path.display()));
    }
    if updated_path != previous_path {
      if let Err(error) = remove_file_if_exists(&previous_path) {
        restore_site_config(&updated_path, updated_config)?;
        if previous.secured {
          let _ = remove_site_certificate(&self.paths, &updated.domain);
        }
        return Err(error).context("unable to remove previous Site config");
      }
    }

    if self.nginx_running() {
      let nginx_config = self.paths.services.join("nginx/nginx.conf");
      let reload_result = validate_nginx_config(&self.runtimes, &nginx_config)
        .and_then(|()| reload_nginx(&self.runtimes, &nginx_config));
      let apply_result = match reload_result {
        Ok(()) => {
          let updated_result = wait_for_site_http_config(
            self.ports.http,
            &updated.domain,
            updated.secured,
            Duration::from_secs(3),
          )
          .await;
          if updated_result.is_ok() && previous.domain != updated.domain {
            wait_for_default_server(self.ports.http, &previous.domain, Duration::from_secs(3)).await
          } else {
            updated_result
          }
        }
        Err(error) => Err(error),
      };
      if let Err(error) = apply_result {
        restore_site_config(&updated_path, updated_config)?;
        if updated_path != previous_path {
          restore_site_config(&previous_path, previous_config)?;
        }
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        if previous.secured && previous.domain != updated.domain {
          let _ = remove_site_certificate(&self.paths, &updated.domain);
        }
        return Err(error.context("unable to apply Site update to Nginx"));
      }
    }

    if previous.secured && previous.domain != updated.domain {
      remove_site_certificate(&self.paths, &previous.domain)?;
    }
    Ok(())
  }

  pub async fn update_site_php_config(
    &mut self,
    previous: &Site,
    updated: &Site,
    sites_after_update: &[Site],
  ) -> Result<()> {
    if previous.php_version == updated.php_version {
      return Ok(());
    }
    self.paths.ensure()?;
    let php_config = updated
      .php_version
      .as_ref()
      .map(|version| generate_php_config(&self.paths, &self.runtimes, version))
      .transpose()?;
    let (config_path, rendered_config) =
      render_site_config(&self.paths, &self.runtimes, self.ports, updated)?;
    let existing_config = std::fs::read(&config_path)
      .with_context(|| format!("unable to read Site config at {}", config_path.display()))?;
    std::fs::write(&config_path, rendered_config)
      .with_context(|| format!("unable to write Site config at {}", config_path.display()))?;

    if self.nginx_running() {
      let php_started = match &php_config {
        Some(config) => {
          if let Err(error) = validate_php_config(config) {
            std::fs::write(&config_path, &existing_config)?;
            return Err(error.context("invalid target PHP-FPM configuration"));
          }
          match self.ensure_php_version_running(config).await {
            Ok(started) => started,
            Err(error) => {
              std::fs::write(&config_path, &existing_config)?;
              return Err(error.context("unable to start target PHP-FPM Runtime"));
            }
          }
        }
        None => false,
      };
      let nginx_config = self.paths.services.join("nginx/nginx.conf");
      let reload_result = validate_nginx_config(&self.runtimes, &nginx_config)
        .and_then(|()| reload_nginx(&self.runtimes, &nginx_config));
      if let Err(error) = reload_result {
        std::fs::write(&config_path, &existing_config)?;
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        if php_started {
          if let Some(version) = &updated.php_version {
            let _ = self.stop_php_version(version).await;
          }
        }
        return Err(error.context("unable to switch Site PHP Runtime in Nginx"));
      }
    }

    if let Some(previous_version) = &previous.php_version {
      let previous_still_used = sites_after_update
        .iter()
        .any(|site| site.enabled && site.php_version.as_ref() == Some(previous_version));
      if !previous_still_used {
        self.stop_php_version(previous_version).await?;
      }
    }
    Ok(())
  }

  pub async fn update_site_https_config(&mut self, previous: &Site, updated: &Site) -> Result<()> {
    if previous.secured == updated.secured {
      return Ok(());
    }
    self.paths.ensure()?;
    let (config_path, rendered_config) =
      render_site_config(&self.paths, &self.runtimes, self.ports, updated)?;
    let existing_config = std::fs::read(&config_path)
      .with_context(|| format!("unable to read Site config at {}", config_path.display()))?;
    std::fs::write(&config_path, rendered_config)
      .with_context(|| format!("unable to write Site config at {}", config_path.display()))?;

    if self.nginx_running() {
      let nginx_config = self.paths.services.join("nginx/nginx.conf");
      let reload_result = validate_nginx_config(&self.runtimes, &nginx_config)
        .and_then(|()| reload_nginx(&self.runtimes, &nginx_config));
      let apply_result = match reload_result {
        Ok(()) => {
          wait_for_site_http_config(
            self.ports.http,
            &updated.domain,
            updated.secured,
            Duration::from_secs(3),
          )
          .await
        }
        Err(error) => Err(error),
      };
      if let Err(error) = apply_result {
        std::fs::write(&config_path, &existing_config)?;
        let _ = reload_nginx(&self.runtimes, &nginx_config);
        if updated.secured && !previous.secured {
          let _ = remove_site_certificate(&self.paths, &updated.domain);
        }
        return Err(error.context("unable to apply Site HTTPS setting to Nginx"));
      }
    }

    if !updated.secured {
      remove_site_certificate(&self.paths, &updated.domain)?;
    }
    Ok(())
  }

  pub fn read_php_ini(&self, version: &PhpVersion) -> Result<String> {
    let config = generate_php_config(&self.paths, &self.runtimes, version)?;
    std::fs::read_to_string(managed_php_ini_path(&self.paths, version))
      .with_context(|| format!("unable to read PHP {} configuration", config.version))
  }

  pub fn read_default_php_ini(&self) -> Result<String> {
    let path = ensure_default_php_ini_template(&self.paths, &self.runtimes)?;
    std::fs::read_to_string(&path).with_context(|| {
      format!(
        "unable to read default PHP configuration at {}",
        path.display()
      )
    })
  }

  pub fn save_default_php_ini(&self, contents: &str) -> Result<()> {
    validate_php_ini_contents(contents)?;
    let path = default_php_ini_path(&self.paths);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contents).with_context(|| {
      format!(
        "unable to save default PHP configuration at {}",
        path.display()
      )
    })
  }

  pub fn ensure_default_php_ini(&self) -> Result<()> {
    ensure_default_php_ini_template(&self.paths, &self.runtimes).map(|_| ())
  }

  pub fn initialize_empty_php_ini(&self, version: &PhpVersion) -> Result<()> {
    self.runtimes.resolve_php(version)?;
    let path = managed_php_ini_path(&self.paths, version);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, "")?;
    generate_php_config(&self.paths, &self.runtimes, version).map(|_| ())
  }

  pub fn validate_php_runtime_install(
    &self,
    version: &PhpVersion,
    expected_patch_version: &str,
  ) -> Result<()> {
    self.runtimes.resolve_php(version)?;
    let managed_php_ini = managed_php_ini_path(&self.paths, version);
    if !managed_php_ini.exists() {
      if let Some(parent) = managed_php_ini.parent() {
        std::fs::create_dir_all(parent)?;
      }
      std::fs::write(&managed_php_ini, "")?;
    }
    let config = generate_php_config(&self.paths, &self.runtimes, version)?;
    validate_php_config(&config)?;
    validate_php_cli(&config, expected_patch_version)
  }

  pub fn read_erp_php_ini(&self, version: Option<&PhpVersion>) -> Result<String> {
    let template_path = ensure_default_php_ini_template(&self.paths, &self.runtimes)?;
    let template = std::fs::read_to_string(template_path)?;
    let Some(version) = version else {
      return Ok(template);
    };

    let runtime = self.runtimes.resolve_php(version)?;
    let service = php_service_path(&self.paths, version);
    let mariadb_settings = self.mariadb_settings()?;
    let mariadb_socket = effective_mariadb_php_socket(&self.paths, &mariadb_settings);
    #[cfg(unix)]
    let extension_api = resolve_php_extension_api(&runtime)?;
    #[cfg(windows)]
    let extension_api = String::new();

    Ok(
      template
        .replace("@RUNTIME_ROOT@", runtime.to_string_lossy().as_ref())
        .replace("@SERVICE_ROOT@", service.to_string_lossy().as_ref())
        .replace(
          "@MARIADB_SOCKET@",
          mariadb_socket.to_string_lossy().as_ref(),
        )
        .replace("@PHP_EXTENSION_API@", &extension_api),
    )
  }

  pub async fn save_php_ini(&mut self, version: &PhpVersion, contents: &str) -> Result<()> {
    validate_php_ini_contents(contents)?;
    generate_php_config(&self.paths, &self.runtimes, version)?;
    let managed_path = managed_php_ini_path(&self.paths, version);
    let previous = std::fs::read(&managed_path)?;
    std::fs::write(&managed_path, contents)?;
    let config = generate_php_config(&self.paths, &self.runtimes, version)?;
    if let Err(error) = validate_php_config(&config) {
      std::fs::write(&managed_path, &previous)?;
      generate_php_config(&self.paths, &self.runtimes, version)?;
      return Err(error.context("invalid php.ini"));
    }

    if self.expected_php_versions.contains(version) {
      self.stop_php_version(version).await?;
      if let Err(error) = self.ensure_php_version_running(&config).await {
        std::fs::write(&managed_path, &previous)?;
        let restored_config = generate_php_config(&self.paths, &self.runtimes, version)?;
        let _ = self.ensure_php_version_running(&restored_config).await;
        return Err(error.context("unable to restart PHP-FPM with updated php.ini"));
      }
    }
    Ok(())
  }

  fn nginx_running(&mut self) -> bool {
    matches!(
      child_state(&mut self.nginx, nginx_binary(&self.runtimes.nginx)),
      ServiceState::Running
    )
  }

  async fn ensure_php_version_running(&mut self, config: &GeneratedPhpConfig) -> Result<bool> {
    if let Some(process) = self.php_fpm.get_mut(&config.version) {
      if process.try_wait()?.is_none() {
        self.expected_php_versions.insert(config.version.clone());
        return Ok(false);
      }
      self.php_fpm.remove(&config.version);
    }

    if let FastCgiEndpoint::UnixSocket(socket) = &config.fastcgi_endpoint {
      remove_file_if_exists(socket)?;
    }
    let child = spawn_php_fpm(config, &self.paths.logs)?;
    self.php_fpm.insert(config.version.clone(), child);
    self.expected_php_versions.insert(config.version.clone());
    if let Err(error) = wait_for_fastcgi(&config.fastcgi_endpoint, Duration::from_secs(5)).await {
      let _ = self.stop_php_version(&config.version).await;
      return Err(error);
    }
    Ok(true)
  }

  async fn stop_php_version(&mut self, version: &PhpVersion) -> Result<()> {
    let mut child = self.php_fpm.remove(version);
    stop_child(&mut child).await?;
    self.expected_php_versions.remove(version);
    let php_service = php_service_path(&self.paths, version);
    remove_file_if_exists(&php_service.join("php-fpm.pid"))?;
    remove_file_if_exists(&php_service.join("php-fpm.sock"))?;
    Ok(())
  }

  async fn restart_php_versions(&mut self, versions: &BTreeSet<PhpVersion>) -> Result<()> {
    let configs = versions
      .iter()
      .map(|version| generate_php_config(&self.paths, &self.runtimes, version))
      .collect::<Result<Vec<_>>>()?;
    for config in &configs {
      validate_php_config(config)?;
    }
    for version in versions {
      self.stop_php_version(version).await?;
    }
    for config in &configs {
      self.ensure_php_version_running(config).await?;
    }
    Ok(())
  }
}

pub fn generate_configs(
  paths: &AppPaths,
  runtimes: &RuntimePaths,
  ports: ServicePorts,
  sites: &[Site],
) -> Result<GeneratedConfigs> {
  if sites.is_empty() {
    bail!("at least one enabled Site is required");
  }
  paths.ensure()?;
  let nginx_service = paths.services.join("nginx");
  std::fs::create_dir_all(&nginx_service)?;
  #[cfg(windows)]
  ensure_windows_nginx_work_directories(&runtimes.nginx)?;

  let nginx_config = nginx_service.join("nginx.conf");
  let dnsmasq_config = paths.services.join("dnsmasq.conf");
  let php_versions = sites
    .iter()
    .filter_map(|site| site.php_version.clone())
    .collect::<BTreeSet<_>>();
  let php = php_versions
    .iter()
    .map(|version| generate_php_config(paths, runtimes, version))
    .collect::<Result<Vec<_>>>()?;

  let global_nginx = NGINX_CONFIG_TEMPLATE
    .replace(
      "@FABDEV_ROOT@",
      &escape_nginx_quoted_value(&paths.root.to_string_lossy()),
    )
    .replace(
      "@NGINX_ROOT@",
      &escape_nginx_quoted_value(&runtimes.nginx.to_string_lossy()),
    )
    .replace("@HTTP_PORT@", &ports.http.to_string())
    .replace("@HTTPS_PORT@", &ports.https.to_string());
  std::fs::write(&nginx_config, global_nginx)?;
  for site in sites {
    let (config_path, rendered_config) = render_site_config(paths, runtimes, ports, site)?;
    std::fs::write(config_path, rendered_config)?;
  }

  let dnsmasq = format!(
    "port={}\nlisten-address=127.0.0.1\nbind-interfaces\nno-resolv\naddress=/.test/127.0.0.1\npid-file={}\nlog-facility={}\n",
    ports.dns,
    paths.services.join("dnsmasq.pid").display(),
    paths.logs.join("dnsmasq.log").display()
  );
  std::fs::write(&dnsmasq_config, dnsmasq)?;

  Ok(GeneratedConfigs {
    dnsmasq: dnsmasq_config,
    nginx: nginx_config,
    php,
  })
}

#[cfg(any(windows, test))]
fn ensure_windows_nginx_work_directories(runtime: &Path) -> Result<()> {
  for directory in ["logs", "temp"] {
    std::fs::create_dir_all(runtime.join(directory)).with_context(|| {
      format!(
        "unable to create Windows Nginx work directory: {}",
        runtime.join(directory).display()
      )
    })?;
  }
  Ok(())
}

pub fn generate_mariadb_config(
  paths: &AppPaths,
  runtime: &Path,
  port: u16,
) -> Result<GeneratedMariaDbConfig> {
  generate_mariadb_config_with_settings(
    paths,
    runtime,
    &MariaDbSettings {
      port,
      data_dir: paths.services.join("mariadb/data"),
      connection_mode: MariaDbConnectionMode::Managed,
      system_socket: default_mariadb_system_socket(),
    },
  )
}

fn generate_mariadb_config_with_settings(
  paths: &AppPaths,
  runtime: &Path,
  settings: &MariaDbSettings,
) -> Result<GeneratedMariaDbConfig> {
  paths.ensure()?;
  let service = paths.services.join("mariadb");
  std::fs::create_dir_all(&service)?;
  let data = settings.data_dir.clone();
  std::fs::create_dir_all(&data)?;
  let config = service.join("my.cnf");
  let pid = service.join("mariadb.pid");
  let socket = service.join("mariadb.sock");
  let custom_config = std::fs::read_to_string(ensure_mariadb_custom_config(paths)?)?;
  let rendered = render_mariadb_config(paths, runtime, settings, &custom_config);
  std::fs::write(&config, rendered)?;
  Ok(GeneratedMariaDbConfig {
    config,
    data,
    pid,
    socket,
  })
}

fn render_mariadb_config(
  paths: &AppPaths,
  runtime: &Path,
  settings: &MariaDbSettings,
  custom_config: &str,
) -> String {
  let service = paths.services.join("mariadb");
  let managed = MARIADB_CONFIG_TEMPLATE
    .replace(
      "@RUNTIME_ROOT@",
      &escape_mariadb_quoted_value(&runtime.to_string_lossy()),
    )
    .replace(
      "@DATA_ROOT@",
      &escape_mariadb_quoted_value(&settings.data_dir.to_string_lossy()),
    )
    .replace("@MARIADB_PORT@", &settings.port.to_string())
    .replace(
      "@MARIADB_SOCKET@",
      &escape_mariadb_quoted_value(&service.join("mariadb.sock").to_string_lossy()),
    )
    .replace(
      "@MARIADB_PID@",
      &escape_mariadb_quoted_value(&service.join("mariadb.pid").to_string_lossy()),
    )
    .replace(
      "@MARIADB_LOG@",
      &escape_mariadb_quoted_value(&paths.logs.join("mariadb-error.log").to_string_lossy()),
    )
    .replace(
      "@MARIADB_PLATFORM_OPTIONS@",
      mariadb_platform_config_options(),
    );
  format!(
    "# User configuration ({})\n{}\n# fabDev managed configuration; managed values below take precedence.\n{}",
    mariadb_config_filename(),
    custom_config.trim_end(),
    managed
  )
}

fn mariadb_platform_config_options() -> &'static str {
  #[cfg(windows)]
  {
    "skip-ssl"
  }
  #[cfg(not(windows))]
  {
    ""
  }
}

fn mariadb_config_filename() -> &'static str {
  if cfg!(windows) {
    "my.ini"
  } else {
    "my.cnf"
  }
}

fn mariadb_custom_config_path(paths: &AppPaths) -> PathBuf {
  paths.config.join("mariadb").join(mariadb_config_filename())
}

fn ensure_mariadb_custom_config(paths: &AppPaths) -> Result<PathBuf> {
  paths.ensure()?;
  let path = mariadb_custom_config_path(paths);
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  if !path.is_file() {
    std::fs::write(&path, MARIADB_CUSTOM_CONFIG_TEMPLATE)
      .with_context(|| format!("unable to create MariaDB configuration: {}", path.display()))?;
  }
  Ok(path)
}

fn validate_mariadb_custom_config(contents: &str) -> Result<()> {
  if contents.is_empty() || contents.len() > MARIADB_CONFIG_MAX_BYTES || contents.contains('\0') {
    bail!("MariaDB configuration must contain between 1 byte and 512 KiB of text");
  }

  const MANAGED_KEYS: &[&str] = &[
    "basedir",
    "bind-address",
    "datadir",
    "host",
    "lc-messages-dir",
    "log-error",
    "named-pipe",
    "pid-file",
    "plugin-dir",
    "port",
    "shared-memory",
    "skip-grant-tables",
    "skip-name-resolve",
    "skip-networking",
    "skip-ssl",
    "socket",
    "ssl",
    "user",
  ];
  for (index, line) in contents.lines().enumerate() {
    let trimmed = line.trim();
    if trimmed.to_ascii_lowercase().starts_with("!include") {
      bail!(
        "MariaDB configuration line {} cannot include another file",
        index + 1
      );
    }
    if trimmed.is_empty()
      || trimmed.starts_with('#')
      || trimmed.starts_with(';')
      || trimmed.starts_with('[')
    {
      continue;
    }
    let key = trimmed
      .split_once('=')
      .map(|(key, _)| key)
      .unwrap_or_else(|| trimmed.split_whitespace().next().unwrap_or_default())
      .trim_start_matches("--")
      .trim()
      .to_ascii_lowercase()
      .replace('_', "-");
    let managed_key = key.strip_prefix("loose-").unwrap_or(&key);
    if MANAGED_KEYS.contains(&managed_key) {
      bail!(
        "MariaDB configuration line {} uses fabDev-managed option: {}",
        index + 1,
        managed_key
      );
    }
  }
  Ok(())
}

fn save_mariadb_custom_config(paths: &AppPaths, contents: &str) -> Result<()> {
  let path = ensure_mariadb_custom_config(paths)?;
  let pending = path.with_extension(format!(
    "{}.pending",
    path
      .extension()
      .and_then(|value| value.to_str())
      .unwrap_or("config")
  ));
  std::fs::write(&pending, contents).with_context(|| {
    format!(
      "unable to write MariaDB configuration: {}",
      pending.display()
    )
  })?;
  #[cfg(windows)]
  if path.is_file() {
    std::fs::remove_file(&path).with_context(|| {
      format!(
        "unable to replace MariaDB configuration: {}",
        path.display()
      )
    })?;
  }
  std::fs::rename(&pending, &path).with_context(|| {
    format!(
      "unable to activate MariaDB configuration: {}",
      path.display()
    )
  })?;
  Ok(())
}

fn mariadb_settings_path(paths: &AppPaths) -> PathBuf {
  paths.config.join("mariadb.json")
}

fn mariadb_connection_settings_path(paths: &AppPaths) -> PathBuf {
  paths.config.join("mariadb-connection.json")
}

fn mariadb_desired_state_path(paths: &AppPaths) -> PathBuf {
  paths.state.join("mariadb.json")
}

fn load_mariadb_desired_state(paths: &AppPaths) -> Result<bool> {
  let path = mariadb_desired_state_path(paths);
  let file = match File::open(&path) {
    Ok(file) => file,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| format!("unable to open {}", path.display()));
    }
  };
  serde_json::from_reader(file)
    .with_context(|| format!("invalid MariaDB service state: {}", path.display()))
}

fn save_mariadb_desired_state(paths: &AppPaths, running: bool) -> Result<()> {
  paths.ensure()?;
  let path = mariadb_desired_state_path(paths);
  let pending = paths.state.join(".mariadb.json.pending");
  let mut contents = serde_json::to_vec_pretty(&running)?;
  contents.push(b'\n');
  std::fs::write(&pending, contents).with_context(|| {
    format!(
      "unable to write MariaDB service state: {}",
      pending.display()
    )
  })?;
  #[cfg(windows)]
  if path.is_file() {
    std::fs::remove_file(&path).with_context(|| {
      format!(
        "unable to replace MariaDB service state: {}",
        path.display()
      )
    })?;
  }
  std::fs::rename(&pending, &path).with_context(|| {
    format!(
      "unable to activate MariaDB service state: {}",
      path.display()
    )
  })?;
  Ok(())
}

fn default_mariadb_settings(paths: &AppPaths, port: u16) -> MariaDbSettings {
  MariaDbSettings {
    port,
    data_dir: default_mariadb_data_dir(paths),
    connection_mode: MariaDbConnectionMode::Managed,
    system_socket: default_mariadb_system_socket(),
  }
}

fn default_mariadb_data_dir(paths: &AppPaths) -> PathBuf {
  paths.services.join("mariadb/data")
}

fn load_mariadb_settings(paths: &AppPaths, default_port: u16) -> Result<MariaDbSettings> {
  let path = mariadb_settings_path(paths);
  let (mut settings, settings_file_exists) = match File::open(&path) {
    Ok(file) => (
      serde_json::from_reader(file)
        .with_context(|| format!("invalid MariaDB settings: {}", path.display()))?,
      true,
    ),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      (default_mariadb_settings(paths, default_port), false)
    }
    Err(error) => {
      return Err(error).with_context(|| format!("unable to open {}", path.display()));
    }
  };
  let connection_path = mariadb_connection_settings_path(paths);
  match File::open(&connection_path) {
    Ok(file) => {
      let connection: MariaDbConnectionSettings =
        serde_json::from_reader(file).with_context(|| {
          format!(
            "invalid MariaDB connection settings: {}",
            connection_path.display()
          )
        })?;
      settings.connection_mode = connection.connection_mode;
      settings.system_socket = connection.system_socket;
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => {
      return Err(error).with_context(|| format!("unable to open {}", connection_path.display()));
    }
  }
  if !settings_file_exists && settings.connection_mode == MariaDbConnectionMode::Managed {
    return Ok(settings);
  }
  restore_missing_default_mariadb_data_dir(paths, &mut settings)?;
  restore_incomplete_default_mariadb_initialization(paths, &settings, cfg!(windows))?;
  validate_mariadb_settings(settings)
}

fn restore_missing_default_mariadb_data_dir(
  paths: &AppPaths,
  settings: &mut MariaDbSettings,
) -> Result<()> {
  if settings.connection_mode != MariaDbConnectionMode::Managed || settings.data_dir.exists() {
    return Ok(());
  }
  let default_data_dir = default_mariadb_data_dir(paths);
  if !mariadb_data_dirs_match(&settings.data_dir, &default_data_dir) {
    return Ok(());
  }
  std::fs::create_dir_all(&default_data_dir).with_context(|| {
    format!(
      "unable to restore the default MariaDB data directory: {}",
      user_visible_path(&default_data_dir)
    )
  })?;
  settings.data_dir = default_data_dir;
  Ok(())
}

fn mariadb_data_dirs_match(left: &Path, right: &Path) -> bool {
  mariadb_data_dirs_match_for_platform(left, right, cfg!(windows))
}

fn mariadb_data_dirs_match_for_platform(left: &Path, right: &Path, windows_paths: bool) -> bool {
  if windows_paths {
    return normalize_windows_path(left) == normalize_windows_path(right);
  }
  left == right
}

fn restore_incomplete_default_mariadb_initialization(
  paths: &AppPaths,
  settings: &MariaDbSettings,
  windows_paths: bool,
) -> Result<()> {
  if !windows_paths || settings.connection_mode != MariaDbConnectionMode::Managed {
    return Ok(());
  }
  let default_data_dir = default_mariadb_data_dir(paths);
  if !mariadb_data_dirs_match_for_platform(&settings.data_dir, &default_data_dir, windows_paths)
    || !settings.data_dir.is_dir()
  {
    return Ok(());
  }
  let mut entries = std::fs::read_dir(&settings.data_dir)?;
  let Some(entry) = entries.next().transpose()? else {
    return Ok(());
  };
  if entries.next().transpose()?.is_some()
    || !entry
      .file_name()
      .to_string_lossy()
      .eq_ignore_ascii_case("my.ini")
    || !entry.file_type()?.is_file()
    || entry.metadata()?.len() > 4096
  {
    return Ok(());
  }
  let contents = std::fs::read_to_string(entry.path())?;
  if !is_windows_mariadb_install_db_stub(paths, &default_data_dir, &contents) {
    return Ok(());
  }
  std::fs::remove_file(entry.path()).with_context(|| {
    format!(
      "unable to remove incomplete MariaDB initialization file: {}",
      user_visible_path(&entry.path())
    )
  })?;
  Ok(())
}

fn is_windows_mariadb_install_db_stub(
  paths: &AppPaths,
  default_data_dir: &Path,
  contents: &str,
) -> bool {
  let mut lines = contents
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty());
  let matches = lines.next() == Some("[mysqld]")
    && lines
      .next()
      .and_then(|line| line.strip_prefix("datadir="))
      .is_some_and(|value| {
        mariadb_data_dirs_match_for_platform(Path::new(value), default_data_dir, true)
      })
    && lines.next() == Some("[client]")
    && lines
      .next()
      .and_then(|line| line.strip_prefix("plugin-dir="))
      .is_some_and(|value| {
        let plugin_dir = normalize_windows_path(Path::new(value));
        let runtime_root = normalize_windows_path(&paths.runtimes.join("mariadb"));
        windows_path_is_under(&plugin_dir, &runtime_root) && plugin_dir.ends_with("/lib/plugin")
      });
  matches && lines.next().is_none()
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MariaDbConnectionSettings {
  connection_mode: MariaDbConnectionMode,
  system_socket: PathBuf,
}

fn validate_mariadb_settings(mut settings: MariaDbSettings) -> Result<MariaDbSettings> {
  if settings.connection_mode == MariaDbConnectionMode::System {
    if !settings.system_socket.is_absolute() || settings.system_socket.parent().is_none() {
      bail!("System MariaDB Socket must be an absolute path");
    }
    return Ok(settings);
  }
  if settings.port < 1024 {
    bail!("MariaDB port must be between 1024 and 65535");
  }
  if !settings.data_dir.is_absolute() {
    bail!("MariaDB data directory must be an absolute path");
  }
  let data_dir = settings.data_dir.canonicalize().with_context(|| {
    format!(
      "MariaDB data directory does not exist: {}",
      user_visible_path(&settings.data_dir)
    )
  })?;
  if !data_dir.is_dir() {
    bail!(
      "MariaDB data path is not a directory: {}",
      user_visible_path(&data_dir)
    );
  }
  if data_dir.parent().is_none() {
    bail!("MariaDB data directory cannot be a filesystem root");
  }
  let has_entries = std::fs::read_dir(&data_dir)?.next().transpose()?.is_some();
  if has_entries && !data_dir.join("mysql").is_dir() {
    bail!(
      "MariaDB data directory must be empty or contain an existing MariaDB database: {}",
      user_visible_path(&data_dir)
    );
  }
  settings.data_dir = data_dir;
  Ok(settings)
}

fn effective_mariadb_php_socket(paths: &AppPaths, settings: &MariaDbSettings) -> PathBuf {
  let managed_socket = paths.services.join("mariadb/mariadb.sock");
  if managed_mariadb_connection_available(paths, settings) {
    return managed_socket;
  }
  detect_system_mariadb_socket(&settings.system_socket)
}

#[cfg(unix)]
fn managed_mariadb_connection_available(paths: &AppPaths, _settings: &MariaDbSettings) -> bool {
  use std::os::unix::fs::FileTypeExt;

  std::fs::metadata(paths.services.join("mariadb/mariadb.sock"))
    .is_ok_and(|metadata| metadata.file_type().is_socket())
}

#[cfg(windows)]
fn managed_mariadb_connection_available(paths: &AppPaths, settings: &MariaDbSettings) -> bool {
  let pid_is_valid = std::fs::read_to_string(paths.services.join("mariadb/mariadb.pid"))
    .ok()
    .and_then(|pid| pid.trim().parse::<u32>().ok())
    .is_some_and(|pid| pid > 0);
  pid_is_valid
    && TcpStream::connect_timeout(
      &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, settings.port)),
      Duration::from_millis(100),
    )
    .is_ok()
}

#[cfg(unix)]
fn detect_system_mariadb_socket(configured: &Path) -> PathBuf {
  let candidates = [
    configured.to_path_buf(),
    default_mariadb_system_socket(),
    PathBuf::from("/opt/homebrew/var/mysql/mysql.sock"),
    PathBuf::from("/usr/local/var/mysql/mysql.sock"),
  ];
  first_existing_unix_socket(&candidates).unwrap_or_else(|| configured.to_path_buf())
}

#[cfg(unix)]
fn first_existing_unix_socket(candidates: &[PathBuf]) -> Option<PathBuf> {
  use std::os::unix::fs::FileTypeExt;

  candidates.iter().find_map(|candidate| {
    std::fs::metadata(candidate)
      .ok()
      .filter(|metadata| metadata.file_type().is_socket())
      .map(|_| candidate.clone())
  })
}

#[cfg(windows)]
fn detect_system_mariadb_socket(configured: &Path) -> PathBuf {
  configured.to_path_buf()
}

fn save_mariadb_settings_file(paths: &AppPaths, settings: &MariaDbSettings) -> Result<()> {
  paths.ensure()?;
  let path = mariadb_settings_path(paths);
  let pending = paths.config.join(".mariadb.json.pending");
  let mut contents = serde_json::to_vec_pretty(settings)?;
  contents.push(b'\n');
  std::fs::write(&pending, contents)
    .with_context(|| format!("unable to write MariaDB settings: {}", pending.display()))?;
  #[cfg(windows)]
  if path.is_file() {
    std::fs::remove_file(&path)
      .with_context(|| format!("unable to replace MariaDB settings: {}", path.display()))?;
  }
  std::fs::rename(&pending, &path)
    .with_context(|| format!("unable to activate MariaDB settings: {}", path.display()))?;
  let connection_path = mariadb_connection_settings_path(paths);
  let connection_pending = paths.config.join(".mariadb-connection.json.pending");
  let connection = MariaDbConnectionSettings {
    connection_mode: settings.connection_mode.clone(),
    system_socket: settings.system_socket.clone(),
  };
  let mut connection_contents = serde_json::to_vec_pretty(&connection)?;
  connection_contents.push(b'\n');
  std::fs::write(&connection_pending, connection_contents).with_context(|| {
    format!(
      "unable to write MariaDB connection settings: {}",
      connection_pending.display()
    )
  })?;
  #[cfg(windows)]
  if connection_path.is_file() {
    std::fs::remove_file(&connection_path).with_context(|| {
      format!(
        "unable to replace MariaDB connection settings: {}",
        connection_path.display()
      )
    })?;
  }
  std::fs::rename(&connection_pending, &connection_path).with_context(|| {
    format!(
      "unable to activate MariaDB connection settings: {}",
      connection_path.display()
    )
  })?;
  Ok(())
}

fn escape_mariadb_quoted_value(value: &str) -> String {
  value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn mariadb_password_statement(password: &str) -> String {
  let escaped = password.replace('\'', "''");
  format!(
    "SET SESSION sql_mode = 'NO_BACKSLASH_ESCAPES';\nALTER USER 'root'@'127.0.0.1' IDENTIFIED BY '{escaped}', 'root'@'localhost' IDENTIFIED BY '{escaped}';\n"
  )
}

fn mariadb_password_error(stderr: &[u8]) -> String {
  let message = String::from_utf8_lossy(stderr);
  if message.contains("ERROR 1045") || message.contains("Access denied") {
    return "unable to authenticate as MariaDB root; verify the current password".to_owned();
  }
  let code = message.lines().find_map(|line| {
    let (_, suffix) = line.split_once("ERROR ")?;
    suffix.split_whitespace().next()
  });
  match code {
    Some(code) => format!("MariaDB rejected the root password change (error {code})"),
    None => "MariaDB rejected the root password change".to_owned(),
  }
}

async fn initialize_mariadb(runtime: &Path, config: &GeneratedMariaDbConfig) -> Result<()> {
  let installer = mariadb_install_db_binary(runtime);
  if !installer.is_file() {
    bail!(
      "fabDev MariaDB Runtime does not contain mariadb-install-db: {}",
      installer.display()
    );
  }
  let mut command = mariadb_install_command(runtime, &installer)?;
  let output = command
    .args(mariadb_install_args(config))
    .output()
    .await
    .with_context(|| {
      format!(
        "unable to initialize fabDev MariaDB using {}",
        installer.display()
      )
    })?;
  if output.status.success() {
    Ok(())
  } else {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    bail!("fabDev MariaDB initialization failed: {detail}")
  }
}

fn mariadb_install_command(runtime: &Path, installer: &Path) -> Result<Command> {
  #[cfg(windows)]
  {
    let mut command = Command::new(installer);
    command.current_dir(runtime);
    Ok(command)
  }
  #[cfg(not(windows))]
  {
    let relative_installer = installer.strip_prefix(runtime).with_context(|| {
      format!(
        "MariaDB installer is outside its Runtime: {}",
        installer.display()
      )
    })?;
    let mut command = Command::new("/bin/sh");
    command.current_dir(runtime).arg(relative_installer);
    Ok(command)
  }
}

fn mariadb_install_args(config: &GeneratedMariaDbConfig) -> Vec<String> {
  #[cfg(windows)]
  {
    vec![
      mariadb_install_data_dir_argument(&config.data, true),
      "--silent".to_owned(),
    ]
  }
  #[cfg(not(windows))]
  {
    vec![
      "--no-defaults".to_owned(),
      format!("--datadir={}", config.data.display()),
      "--auth-root-authentication-method=normal".to_owned(),
      "--skip-name-resolve".to_owned(),
      "--skip-test-db".to_owned(),
    ]
  }
}

#[cfg(any(windows, test))]
fn mariadb_install_data_dir_argument(data_dir: &Path, windows_path: bool) -> String {
  let data_dir = if windows_path {
    windows_path_without_verbatim_prefix(data_dir)
  } else {
    data_dir.display().to_string()
  };
  format!("--datadir={data_dir}")
}

fn escape_nginx_quoted_value(value: &str) -> String {
  value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn generate_php_config(
  paths: &AppPaths,
  runtimes: &RuntimePaths,
  version: &PhpVersion,
) -> Result<GeneratedPhpConfig> {
  let runtime = runtimes.resolve_php(version)?;
  let php_service = php_service_path(paths, version);
  std::fs::create_dir_all(php_service.join("logs"))?;
  std::fs::create_dir_all(php_service.join("session"))?;

  #[cfg(unix)]
  std::fs::create_dir_all(php_service.join("php-fpm.d"))?;

  let php_ini = php_service.join("php.ini");
  let php_fpm = php_service.join("php-fpm.conf");
  let php_service_value = php_service.to_string_lossy();
  let php_runtime_value = runtime.to_string_lossy();
  let mariadb_settings = load_mariadb_settings(paths, 3306)?;
  let mariadb_socket = effective_mariadb_php_socket(paths, &mariadb_settings);
  let mariadb_socket_value = mariadb_socket.to_string_lossy();
  #[cfg(unix)]
  let php_extension_api = resolve_php_extension_api(&runtime)?;
  #[cfg(windows)]
  let php_extension_api = String::new();
  let render_php = |template: &str| {
    template
      .replace("@RUNTIME_ROOT@", &php_runtime_value)
      .replace("@SERVICE_ROOT@", &php_service_value)
      .replace("@MARIADB_SOCKET@", &mariadb_socket_value)
      .replace("@PHP_EXTENSION_API@", &php_extension_api)
  };
  let managed_php_ini = managed_php_ini_path(paths, version);
  if let Some(parent) = managed_php_ini.parent() {
    std::fs::create_dir_all(parent)?;
  }
  if !managed_php_ini.exists() {
    let template_path = ensure_default_php_ini_template(paths, runtimes)?;
    let template = std::fs::read_to_string(&template_path)?;
    std::fs::write(&managed_php_ini, render_php(&template))?;
  }
  let managed_php_ini_contents = std::fs::read_to_string(&managed_php_ini)?;
  let service_php_ini_contents =
    effective_php_ini_contents(&managed_php_ini_contents, cfg!(windows), &render_php);
  std::fs::write(&php_ini, service_php_ini_contents)?;
  #[cfg(unix)]
  {
    let php_pool = php_service.join("php-fpm.d/www.conf");
    std::fs::write(&php_fpm, render_php(PHP_FPM_TEMPLATE))?;
    std::fs::write(&php_pool, render_php(PHP_POOL_TEMPLATE))?;
  }

  Ok(GeneratedPhpConfig {
    version: version.clone(),
    runtime,
    php_fpm,
    php_ini,
    php_socket: php_service.join("php-fpm.sock"),
    fastcgi_endpoint: php_fastcgi_endpoint(paths, version),
  })
}

fn php_ini_template(version: &PhpVersion) -> &'static str {
  if cfg!(windows) {
    return PHP_WINDOWS_INI_TEMPLATE;
  }
  match version.to_string().as_str() {
    "7.4" => PHP_74_INI_TEMPLATE,
    "8.2" => PHP_82_INI_TEMPLATE,
    "8.4" => PHP_82_INI_TEMPLATE,
    _ => PHP_INI_TEMPLATE,
  }
}

fn effective_php_ini_contents(
  managed_contents: &str,
  windows: bool,
  render_php: &impl Fn(&str) -> String,
) -> String {
  if windows && managed_contents.is_empty() {
    render_php(PHP_WINDOWS_INI_TEMPLATE)
  } else {
    managed_contents.to_owned()
  }
}

fn validate_php_ini_contents(contents: &str) -> Result<()> {
  if contents.is_empty() || contents.len() > 512 * 1024 || contents.contains('\0') {
    bail!("php.ini must contain between 1 byte and 512 KiB of text");
  }
  Ok(())
}

fn ensure_default_php_ini_template(paths: &AppPaths, runtimes: &RuntimePaths) -> Result<PathBuf> {
  let path = default_php_ini_path(paths);
  if std::fs::metadata(&path)
    .map(|metadata| metadata.len() > 0)
    .unwrap_or(false)
  {
    return Ok(path);
  }
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }

  let version = PhpVersion { major: 8, minor: 2 };
  let current = managed_php_ini_path(paths, &version);
  let template = match (
    std::fs::read_to_string(&current),
    runtimes.resolve_php(&version),
  ) {
    (Ok(contents), Ok(runtime)) if !contents.trim().is_empty() => {
      let service = php_service_path(paths, &version);
      let mut template = contents
        .replace(runtime.to_string_lossy().as_ref(), "@RUNTIME_ROOT@")
        .replace(service.to_string_lossy().as_ref(), "@SERVICE_ROOT@")
        .replace(
          paths
            .services
            .join("mariadb/mariadb.sock")
            .to_string_lossy()
            .as_ref(),
          "@MARIADB_SOCKET@",
        );
      #[cfg(unix)]
      if let Ok(extension_api) = resolve_php_extension_api(&runtime) {
        template = template.replace(&extension_api, "@PHP_EXTENSION_API@");
      }
      template
    }
    _ => php_ini_template(&version).to_owned(),
  };
  std::fs::write(&path, template).with_context(|| {
    format!(
      "unable to initialize default PHP configuration at {}",
      path.display()
    )
  })?;
  Ok(path)
}

fn resolve_php_extension_api(runtime: &Path) -> Result<String> {
  let extension_root = runtime.join("lib/php/extensions");
  let mut candidates = std::fs::read_dir(&extension_root)
    .with_context(|| {
      format!(
        "PHP extension directory is missing: {}",
        extension_root.display()
      )
    })?
    .filter_map(|entry| entry.ok())
    .filter(|entry| entry.path().join("opcache.so").is_file())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect::<Vec<_>>();
  candidates.sort();
  candidates.pop().with_context(|| {
    format!(
      "PHP Runtime does not contain opcache.so: {}",
      runtime.display()
    )
  })
}

fn php_service_path(paths: &AppPaths, version: &PhpVersion) -> PathBuf {
  paths.services.join("php").join(version.to_string())
}

fn managed_php_ini_path(paths: &AppPaths, version: &PhpVersion) -> PathBuf {
  paths
    .config
    .join("php")
    .join(version.to_string())
    .join("php.ini")
}

fn default_php_ini_path(paths: &AppPaths) -> PathBuf {
  paths.config.join("php/default/php.ini")
}

fn php_socket_path(paths: &AppPaths, version: &PhpVersion) -> PathBuf {
  php_service_path(paths, version).join("php-fpm.sock")
}

fn php_fastcgi_endpoint(paths: &AppPaths, version: &PhpVersion) -> FastCgiEndpoint {
  #[cfg(unix)]
  {
    FastCgiEndpoint::UnixSocket(php_socket_path(paths, version))
  }
  #[cfg(windows)]
  {
    let _ = paths;
    let port = 19_000 + u16::from(version.major) * 10 + u16::from(version.minor);
    FastCgiEndpoint::Tcp(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into())
  }
}

fn render_site_config(
  paths: &AppPaths,
  runtimes: &RuntimePaths,
  ports: ServicePorts,
  site: &Site,
) -> Result<(PathBuf, String)> {
  let domain = normalize_domain(&site.domain).context("invalid Site domain in registry")?;
  if domain != site.domain {
    bail!("Site domain is not normalized: {}", site.domain);
  }
  let tls = site
    .secured
    .then(|| ensure_site_certificate(paths, &site.domain))
    .transpose()?
    .map(|certificate| NginxTlsConfig {
      certificate: certificate.certificate,
      private_key: certificate.private_key,
    });
  let rendered = render_nginx_site(&NginxSiteConfig {
    site: site.clone(),
    nginx_root: runtimes.nginx.clone(),
    fastcgi_endpoint: site
      .php_version
      .as_ref()
      .map(|version| php_fastcgi_endpoint(paths, version)),
    listen_port: ports.http,
    https_listen_port: ports.https,
    tls,
  })?;
  Ok((paths.sites.join(format!("{domain}.conf")), rendered))
}

fn restore_site_config(path: &Path, existing_config: Option<Vec<u8>>) -> Result<()> {
  match existing_config {
    Some(config) => std::fs::write(path, config)
      .with_context(|| format!("unable to restore Site config at {}", path.display())),
    None => remove_file_if_exists(path),
  }
}

fn apply_site_config_files<F>(
  affected_paths: &BTreeSet<PathBuf>,
  rendered_configs: &[(PathBuf, String)],
  apply_running_config: bool,
  mut apply: F,
) -> Result<()>
where
  F: FnMut() -> Result<()>,
{
  let snapshots = affected_paths
    .iter()
    .map(|path| {
      let contents = match std::fs::read(path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
      };
      Ok((path.clone(), contents))
    })
    .collect::<Result<BTreeMap<_, _>>>()?;

  let mut apply_attempted = false;
  let update_result = (|| {
    for path in affected_paths {
      remove_file_if_exists(path)
        .with_context(|| format!("unable to remove Site config at {}", path.display()))?;
    }
    for (path, rendered) in rendered_configs {
      std::fs::write(path, rendered)
        .with_context(|| format!("unable to write Site config at {}", path.display()))?;
    }
    if apply_running_config {
      apply_attempted = true;
      apply()?;
    }
    Ok(())
  })();
  let Err(error) = update_result else {
    return Ok(());
  };

  let restore_errors = snapshots
    .into_iter()
    .filter_map(|(path, contents)| {
      restore_site_config(&path, contents)
        .err()
        .map(|error| format!("{}: {error:#}", path.display()))
    })
    .collect::<Vec<_>>();
  let reload_result = if apply_attempted { apply() } else { Ok(()) };
  if restore_errors.is_empty() && reload_result.is_ok() {
    return Err(error);
  }
  let mut rollback_errors = Vec::new();
  if !restore_errors.is_empty() {
    rollback_errors.push(format!(
      "file restore failed: {}",
      restore_errors.join(", ")
    ));
  }
  if let Err(reload_error) = reload_result {
    rollback_errors.push(format!("Nginx restore reload failed: {reload_error:#}"));
  }
  Err(error.context(format!(
    "Site configuration rollback also failed: {}",
    rollback_errors.join("; ")
  )))
}

fn remove_new_site_certificates(paths: &AppPaths, domains: &BTreeSet<String>) {
  for domain in domains {
    if let Err(error) = remove_site_certificate(paths, domain) {
      eprintln!("unable to remove new Site certificate for {domain}: {error:#}");
    }
  }
}

fn validate_configs(runtimes: &RuntimePaths, configs: &GeneratedConfigs) -> Result<()> {
  #[cfg(unix)]
  run_check(
    dnsmasq_binary(&runtimes.dnsmasq),
    [
      "--test".to_owned(),
      format!("--conf-file={}", configs.dnsmasq.display()),
    ],
    "dnsmasq",
  )?;
  for config in &configs.php {
    validate_php_config(config)?;
  }
  validate_nginx_config(runtimes, &configs.nginx)
}

fn validate_php_config(config: &GeneratedPhpConfig) -> Result<()> {
  #[cfg(windows)]
  return run_check(
    php_server_binary(&config.runtime),
    [
      "-c".to_owned(),
      config.php_ini.to_string_lossy().into_owned(),
      "-v".to_owned(),
    ],
    &format!("PHP {} CGI", config.version),
  );

  #[cfg(unix)]
  run_check(
    php_server_binary(&config.runtime),
    [
      "-c".to_owned(),
      config.php_ini.to_string_lossy().into_owned(),
      "-y".to_owned(),
      config.php_fpm.to_string_lossy().into_owned(),
      "-t".to_owned(),
    ],
    &format!("PHP {} FPM", config.version),
  )
}

fn validate_php_cli(config: &GeneratedPhpConfig, expected_patch_version: &str) -> Result<()> {
  let script = format!(
    "if (PHP_VERSION !== '{expected_patch_version}' || !extension_loaded('mysqli') || !extension_loaded('pdo_mysql')) {{ fwrite(STDERR, 'PHP Runtime health check failed'); exit(1); }}"
  );
  run_check(
    php_cli_binary(&config.runtime),
    [
      "-c".to_owned(),
      config.php_ini.to_string_lossy().into_owned(),
      "-r".to_owned(),
      script,
    ],
    &format!("PHP {expected_patch_version} CLI and required extensions"),
  )
}

fn validate_nginx_config(runtimes: &RuntimePaths, config: &Path) -> Result<()> {
  run_check(
    nginx_binary(&runtimes.nginx),
    [
      "-p".to_owned(),
      format!("{}/", runtimes.nginx.display()),
      "-c".to_owned(),
      config.to_string_lossy().into_owned(),
      "-t".to_owned(),
    ],
    "Nginx",
  )
}

fn reload_nginx(runtimes: &RuntimePaths, config: &Path) -> Result<()> {
  run_check(
    nginx_binary(&runtimes.nginx),
    [
      "-p".to_owned(),
      format!("{}/", runtimes.nginx.display()),
      "-c".to_owned(),
      config.to_string_lossy().into_owned(),
      "-s".to_owned(),
      "reload".to_owned(),
    ],
    "Nginx reload",
  )
}

fn run_check(
  executable: impl AsRef<Path>,
  arguments: impl IntoIterator<Item = String>,
  name: &str,
) -> Result<()> {
  let mut command = background_std_command(executable.as_ref().as_os_str());
  let output = command
    .args(arguments)
    .output()
    .with_context(|| format!("unable to validate {name} configuration"))?;
  if output.status.success() {
    Ok(())
  } else {
    bail!(
      "{name} configuration validation failed: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    )
  }
}

fn spawn_service(
  executable: impl AsRef<Path>,
  arguments: impl IntoIterator<Item = String>,
  log_path: impl AsRef<Path>,
) -> Result<Child> {
  let stdout = File::create(log_path.as_ref())?;
  let stderr = stdout.try_clone()?;
  let mut command = background_command(executable.as_ref().as_os_str());
  command
    .args(arguments)
    .stdin(Stdio::null())
    .stdout(Stdio::from(stdout))
    .stderr(Stdio::from(stderr))
    .kill_on_drop(false);
  command
    .spawn()
    .with_context(|| format!("unable to start {}", executable.as_ref().display()))
}

fn spawn_php_fpm(config: &GeneratedPhpConfig, logs: &Path) -> Result<Child> {
  #[cfg(windows)]
  {
    let FastCgiEndpoint::Tcp(address) = &config.fastcgi_endpoint else {
      bail!("Windows PHP CGI requires a TCP FastCGI endpoint");
    };
    let stdout = File::create(logs.join(format!("php-cgi-{}-process.log", config.version)))?;
    let stderr = stdout.try_clone()?;
    let mut command = background_command(php_server_binary(&config.runtime).as_os_str());
    command
      .args([
        "-b".to_owned(),
        address.to_string(),
        "-c".to_owned(),
        config.php_ini.to_string_lossy().into_owned(),
      ])
      .env("PHP_FCGI_MAX_REQUESTS", "0")
      .stdin(Stdio::null())
      .stdout(Stdio::from(stdout))
      .stderr(Stdio::from(stderr))
      .kill_on_drop(false);
    return command
      .spawn()
      .with_context(|| format!("unable to start PHP {} CGI", config.version));
  }

  #[cfg(unix)]
  spawn_service(
    php_server_binary(&config.runtime),
    [
      "-c".to_owned(),
      config.php_ini.to_string_lossy().into_owned(),
      "-y".to_owned(),
      config.php_fpm.to_string_lossy().into_owned(),
      "-F".to_owned(),
      "-O".to_owned(),
    ],
    logs.join(format!("php-fpm-{}-process.log", config.version)),
  )
}

fn background_command(executable: impl AsRef<OsStr>) -> Command {
  let command = Command::new(executable);
  #[cfg(windows)]
  let command = {
    let mut command = command;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    command
  };
  command
}

fn background_std_command(executable: impl AsRef<OsStr>) -> std::process::Command {
  let command = std::process::Command::new(executable);
  #[cfg(windows)]
  let command = {
    use std::os::windows::process::CommandExt;

    let mut command = command;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    command
  };
  command
}

async fn wait_for_path(path: &Path, timeout: Duration) -> Result<()> {
  let started = tokio::time::Instant::now();
  while started.elapsed() < timeout {
    if path.exists() {
      return Ok(());
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  bail!("service did not create expected path: {}", path.display())
}

async fn wait_for_fastcgi(endpoint: &FastCgiEndpoint, timeout: Duration) -> Result<()> {
  match endpoint {
    FastCgiEndpoint::UnixSocket(path) => wait_for_path(path, timeout).await,
    FastCgiEndpoint::Tcp(address) => {
      let started = tokio::time::Instant::now();
      while started.elapsed() < timeout {
        if tokio::net::TcpStream::connect(address).await.is_ok() {
          return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
      }
      bail!("FastCGI service did not listen at {address}")
    }
  }
}

fn dnsmasq_binary(runtime: &Path) -> PathBuf {
  runtime.join("sbin/dnsmasq")
}

fn nginx_binary(runtime: &Path) -> PathBuf {
  if cfg!(windows) {
    runtime.join("nginx.exe")
  } else {
    runtime.join("sbin/nginx")
  }
}

fn php_server_binary(runtime: &Path) -> PathBuf {
  if cfg!(windows) {
    runtime.join("php-cgi.exe")
  } else {
    runtime.join("sbin/php-fpm")
  }
}

fn php_cli_binary(runtime: &Path) -> PathBuf {
  if cfg!(windows) {
    runtime.join("php.exe")
  } else {
    runtime.join("bin/php")
  }
}

fn mariadb_server_binary(runtime: &Path) -> PathBuf {
  if cfg!(windows) {
    runtime.join("bin/mariadbd.exe")
  } else {
    runtime.join("bin/mariadbd")
  }
}

fn mariadb_client_binary(runtime: &Path) -> PathBuf {
  if cfg!(windows) {
    runtime.join("bin/mariadb.exe")
  } else {
    runtime.join("bin/mariadb")
  }
}

fn mariadb_install_db_binary(runtime: &Path) -> PathBuf {
  #[cfg(windows)]
  {
    return runtime.join("bin/mariadb-install-db.exe");
  }
  #[cfg(not(windows))]
  {
    let script = runtime.join("scripts/mariadb-install-db");
    if script.is_file() {
      script
    } else {
      runtime.join("bin/mariadb-install-db")
    }
  }
}

fn ensure_tcp_port_available(port: u16, service: &str) -> Result<()> {
  TcpListener::bind((Ipv4Addr::LOCALHOST, port))
    .map_err(|error| tcp_port_unavailable_error(error, port, service))?;
  Ok(())
}

fn tcp_port_unavailable_error(error: std::io::Error, port: u16, service: &str) -> anyhow::Error {
  anyhow::Error::new(error).context(format!(
    "{service} cannot use 127.0.0.1:{port}; the port is unavailable"
  ))
}

async fn wait_for_tcp_port(port: u16, timeout: Duration) -> Result<()> {
  let started = tokio::time::Instant::now();
  while started.elapsed() < timeout {
    if tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
      .await
      .is_ok()
    {
      return Ok(());
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  bail!("service did not listen at 127.0.0.1:{port}")
}

async fn wait_for_default_server(port: u16, domain: &str, timeout: Duration) -> Result<()> {
  let started = tokio::time::Instant::now();
  while started.elapsed() < timeout {
    if let Ok(mut stream) = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await {
      let request = format!("GET / HTTP/1.1\r\nHost: {domain}\r\nConnection: close\r\n\r\n");
      if stream.write_all(request.as_bytes()).await.is_ok() {
        let mut response = vec![0_u8; 2048];
        if let Ok(Ok(length)) =
          tokio::time::timeout(Duration::from_millis(250), stream.read(&mut response)).await
        {
          let headers = String::from_utf8_lossy(&response[..length]);
          if headers.contains("\r\nX-fabDev-Default: 1\r\n") {
            return Ok(());
          }
        }
      }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  bail!("Nginx default server did not become active for {domain}")
}

async fn wait_for_site_http_config(
  port: u16,
  domain: &str,
  secured: bool,
  timeout: Duration,
) -> Result<()> {
  let started = tokio::time::Instant::now();
  while started.elapsed() < timeout {
    if let Ok(mut stream) = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await {
      let request = format!("GET / HTTP/1.1\r\nHost: {domain}\r\nConnection: close\r\n\r\n");
      if stream.write_all(request.as_bytes()).await.is_ok() {
        let mut response = vec![0_u8; 2048];
        if let Ok(Ok(length)) =
          tokio::time::timeout(Duration::from_millis(250), stream.read(&mut response)).await
        {
          let headers = String::from_utf8_lossy(&response[..length]);
          let is_default = headers.contains("\r\nX-fabDev-Default: 1\r\n");
          let redirects_to_https = headers.starts_with("HTTP/1.1 301 ")
            && headers.contains(&format!("\r\nLocation: https://{domain}/\r\n"));
          if !is_default && redirects_to_https == secured {
            return Ok(());
          }
        }
      }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  bail!("Nginx did not finish applying the HTTPS setting for {domain}")
}

async fn stop_child(child: &mut Option<Child>) -> Result<()> {
  if let Some(mut process) = child.take() {
    if process.try_wait()?.is_none() {
      #[cfg(unix)]
      {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        if let Some(id) = process.id() {
          let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
        }
        if tokio::time::timeout(Duration::from_secs(3), process.wait())
          .await
          .is_err()
        {
          process.kill().await?;
        }
      }
      #[cfg(not(unix))]
      process.kill().await?;
    }
  }
  Ok(())
}

fn collect_stop_error(errors: &mut Vec<String>, target: &str, result: Result<()>) {
  if let Err(error) = result {
    errors.push(format!("{target}: {error}"));
  }
}

fn finish_stop(errors: Vec<String>) -> Result<()> {
  if errors.is_empty() {
    Ok(())
  } else {
    bail!(
      "unable to fully stop fabDev services: {}",
      errors.join("; ")
    )
  }
}

#[cfg(unix)]
fn command_starts_with_path(command: &str, path: &Path) -> bool {
  let path = path.to_string_lossy();
  command
    .strip_prefix(path.as_ref())
    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
}

#[cfg(unix)]
fn command_contains_path(command: &str, path: &Path) -> bool {
  command.contains(path.to_string_lossy().as_ref())
}

#[cfg(unix)]
fn command_starts_inside_path(command: &str, path: &Path) -> bool {
  let path = path.to_string_lossy();
  command
    .strip_prefix(path.as_ref())
    .is_some_and(|suffix| suffix.starts_with(std::path::MAIN_SEPARATOR))
}

#[cfg(unix)]
fn command_contains_managed_php_config(command: &str, paths: &AppPaths) -> bool {
  let php_root = format!("{}/", paths.services.join("php").display());
  command
    .find(&php_root)
    .is_some_and(|position| command[position + php_root.len()..].contains("/php-fpm.conf"))
}

#[cfg(unix)]
fn is_managed_web_process(command: &str, paths: &AppPaths, runtimes: &RuntimePaths) -> bool {
  let dnsmasq = command_starts_with_path(command, &dnsmasq_binary(&runtimes.dnsmasq))
    && command_contains_path(command, &paths.services.join("dnsmasq.conf"));
  let nginx = command_starts_with_path(command, &nginx_binary(&runtimes.nginx))
    && command_contains_path(command, &paths.services.join("nginx/nginx.conf"));
  let php_config = command_contains_managed_php_config(command, paths);
  let php = php_config
    && (command.starts_with("php-fpm: master process (")
      || command_starts_inside_path(command, &runtimes.php));
  dnsmasq || nginx || php
}

#[cfg(unix)]
fn is_managed_mariadb_process(command: &str, paths: &AppPaths, runtimes: &RuntimePaths) -> bool {
  command_starts_with_path(command, &mariadb_server_binary(&runtimes.mariadb))
    && command_contains_path(command, &paths.services.join("mariadb/my.cnf"))
}

#[cfg(unix)]
fn managed_process_ids_from_output(
  output: &str,
  current_pid: u32,
  matcher: impl Fn(&str) -> bool,
) -> Vec<u32> {
  output
    .lines()
    .filter_map(|line| {
      let line = line.trim_start();
      let split = line.find(char::is_whitespace)?;
      let pid = line[..split].parse::<u32>().ok()?;
      let command = line[split..].trim_start();
      (pid != current_pid && matcher(command)).then_some(pid)
    })
    .collect()
}

#[cfg(unix)]
fn managed_process_ids(
  paths: &AppPaths,
  runtimes: &RuntimePaths,
  matcher: impl Fn(&str, &AppPaths, &RuntimePaths) -> bool,
) -> Result<Vec<u32>> {
  let output = std::process::Command::new("/bin/ps")
    .args(["-axo", "pid=,command="])
    .output()
    .context("unable to inspect running processes")?;
  if !output.status.success() {
    bail!(
      "unable to inspect running processes: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(managed_process_ids_from_output(
    &String::from_utf8_lossy(&output.stdout),
    std::process::id(),
    |command| matcher(command, paths, runtimes),
  ))
}

#[cfg(unix)]
async fn stop_untracked_web_processes(paths: &AppPaths, runtimes: &RuntimePaths) -> Result<()> {
  let pids = managed_process_ids(paths, runtimes, |command, paths, runtimes| {
    is_managed_web_process(command, paths, runtimes)
  })?;
  for pid in pids {
    stop_process(pid, "untracked fabDev web service").await?;
  }
  Ok(())
}

#[cfg(not(unix))]
async fn stop_untracked_web_processes(_paths: &AppPaths, runtimes: &RuntimePaths) -> Result<()> {
  #[cfg(windows)]
  {
    stop_windows_processes(|path| is_managed_windows_web_executable(path, runtimes))
  }

  #[cfg(not(windows))]
  Ok(())
}

#[cfg(unix)]
async fn stop_untracked_mariadb_processes(paths: &AppPaths, runtimes: &RuntimePaths) -> Result<()> {
  let pids = managed_process_ids(paths, runtimes, is_managed_mariadb_process)?;
  for pid in pids {
    stop_process(pid, "untracked fabDev MariaDB").await?;
  }
  Ok(())
}

#[cfg(not(unix))]
async fn stop_untracked_mariadb_processes(
  _paths: &AppPaths,
  runtimes: &RuntimePaths,
) -> Result<()> {
  #[cfg(windows)]
  {
    stop_windows_processes(|path| is_managed_windows_mariadb_executable(path, runtimes))
  }

  #[cfg(not(windows))]
  Ok(())
}

fn windows_path_without_verbatim_prefix(path: &Path) -> String {
  let path = path.to_string_lossy();
  path
    .strip_prefix(r"\\?\UNC\")
    .map(|path| format!(r"\\{path}"))
    .or_else(|| {
      path
        .strip_prefix("//?/UNC/")
        .map(|path| format!("//{path}"))
    })
    .or_else(|| path.strip_prefix(r"\\?\").map(str::to_owned))
    .or_else(|| path.strip_prefix("//?/").map(str::to_owned))
    .unwrap_or_else(|| path.into_owned())
}

fn user_visible_path(path: &Path) -> String {
  if cfg!(windows) {
    return windows_path_without_verbatim_prefix(path);
  }
  path.display().to_string()
}

fn normalize_windows_path(path: &Path) -> String {
  windows_path_without_verbatim_prefix(path)
    .replace('\\', "/")
    .to_lowercase()
}

fn windows_path_is_under(path: &str, root: &str) -> bool {
  path
    .strip_prefix(root)
    .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(any(windows, test))]
fn windows_runtime_family_root(runtime: &Path) -> String {
  normalize_windows_path(runtime)
    .strip_suffix("/current")
    .map(str::to_owned)
    .unwrap_or_else(|| normalize_windows_path(runtime))
}

#[cfg(any(windows, test))]
fn is_managed_windows_web_executable(path: &Path, runtimes: &RuntimePaths) -> bool {
  let path = normalize_windows_path(path);
  let nginx_root = windows_runtime_family_root(&runtimes.nginx);
  let php_root = normalize_windows_path(&runtimes.php);
  (windows_path_is_under(&path, &nginx_root) && path.ends_with("/nginx.exe"))
    || (windows_path_is_under(&path, &php_root) && path.ends_with("/php-cgi.exe"))
}

#[cfg(any(windows, test))]
fn is_managed_windows_mariadb_executable(path: &Path, runtimes: &RuntimePaths) -> bool {
  let path = normalize_windows_path(path);
  let mariadb_root = windows_runtime_family_root(&runtimes.mariadb);
  windows_path_is_under(&path, &mariadb_root) && path.ends_with("/mariadbd.exe")
}

#[cfg(windows)]
fn stop_windows_processes(matcher: impl Fn(&Path) -> bool) -> Result<()> {
  use std::ffi::OsString;
  use std::os::windows::ffi::OsStringExt;

  use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
  use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
  };
  use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
  };

  let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
  if snapshot == INVALID_HANDLE_VALUE {
    return Err(std::io::Error::last_os_error()).context("unable to inspect Windows processes");
  }
  let mut entry = PROCESSENTRY32W {
    dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
    ..Default::default()
  };
  let mut process_ids = Vec::new();
  let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
  while has_entry {
    if entry.th32ProcessID != std::process::id() {
      process_ids.push(entry.th32ProcessID);
    }
    has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
  }
  unsafe {
    CloseHandle(snapshot);
  }

  for process_id in process_ids {
    let process = unsafe {
      OpenProcess(
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
        0,
        process_id,
      )
    };
    if process.is_null() {
      continue;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let has_path =
      unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } != 0;
    let path = has_path.then(|| {
      PathBuf::from(OsString::from_wide(
        &buffer[..usize::try_from(length).unwrap_or_default()],
      ))
    });
    if path.as_deref().is_some_and(&matcher) {
      if unsafe { TerminateProcess(process, 1) } == 0 {
        unsafe {
          CloseHandle(process);
        }
        return Err(std::io::Error::last_os_error())
          .with_context(|| format!("unable to stop managed Windows process {process_id}"));
      }
      if unsafe { WaitForSingleObject(process, 3_000) } != WAIT_OBJECT_0 {
        unsafe {
          CloseHandle(process);
        }
        bail!("managed Windows process {process_id} did not stop");
      }
    }
    unsafe {
      CloseHandle(process);
    }
  }
  Ok(())
}

fn remove_web_service_artifacts(paths: &AppPaths) -> Result<()> {
  for path in [
    paths.services.join("nginx/nginx.pid"),
    paths.services.join("dnsmasq.pid"),
  ] {
    remove_file_if_exists(&path)?;
  }
  let php_root = paths.services.join("php");
  let entries = match std::fs::read_dir(&php_root) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => {
      return Err(error).with_context(|| {
        format!(
          "unable to list PHP service directory: {}",
          php_root.display()
        )
      })
    }
  };
  for entry in entries.filter_map(|entry| entry.ok()) {
    let service = entry.path();
    remove_file_if_exists(&service.join("php-fpm.pid"))?;
    remove_file_if_exists(&service.join("php-fpm.sock"))?;
  }
  Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn rotate_managed_logs(paths: &AppPaths, max_bytes: u64, retention: usize) -> Result<()> {
  if retention == 0 {
    bail!("managed log retention must be greater than zero");
  }
  let mut logs = Vec::new();
  collect_log_files(&paths.logs, &mut logs)?;
  let php_root = paths.services.join("php");
  match std::fs::read_dir(&php_root) {
    Ok(entries) => {
      for entry in entries.filter_map(|entry| entry.ok()) {
        collect_log_files(&entry.path().join("logs"), &mut logs)?;
      }
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => {
      return Err(error)
        .with_context(|| format!("unable to list PHP logs at {}", php_root.display()))
    }
  }
  for log in logs {
    rotate_log_file(&log, max_bytes, retention)?;
  }
  Ok(())
}

fn collect_log_files(directory: &Path, logs: &mut Vec<PathBuf>) -> Result<()> {
  let entries = match std::fs::read_dir(directory) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => {
      return Err(error)
        .with_context(|| format!("unable to list managed logs at {}", directory.display()))
    }
  };
  for entry in entries.filter_map(|entry| entry.ok()) {
    let path = entry.path();
    if path.is_file() && path.extension().is_some_and(|extension| extension == "log") {
      logs.push(path);
    }
  }
  Ok(())
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
  let filename = path.file_name().unwrap_or_default().to_string_lossy();
  path.with_file_name(format!("{filename}.{index}"))
}

fn rotate_log_file(path: &Path, max_bytes: u64, retention: usize) -> Result<bool> {
  let metadata = match std::fs::metadata(path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => return Err(error.into()),
  };
  if metadata.len() < max_bytes {
    return Ok(false);
  }
  remove_file_if_exists(&rotated_log_path(path, retention))?;
  for index in (1..retention).rev() {
    let source = rotated_log_path(path, index);
    if !source.exists() {
      continue;
    }
    let destination = rotated_log_path(path, index + 1);
    remove_file_if_exists(&destination)?;
    std::fs::rename(&source, &destination).with_context(|| {
      format!(
        "unable to rotate managed log {} to {}",
        source.display(),
        destination.display()
      )
    })?;
  }
  let archive = rotated_log_path(path, 1);
  std::fs::copy(path, &archive).with_context(|| {
    format!(
      "unable to archive managed log {} to {}",
      path.display(),
      archive.display()
    )
  })?;
  OpenOptions::new()
    .write(true)
    .truncate(true)
    .open(path)
    .with_context(|| format!("unable to truncate managed log at {}", path.display()))?;
  Ok(true)
}

fn child_state(child: &mut Option<Child>, executable: PathBuf) -> ServiceState {
  if let Some(process) = child {
    match process.try_wait() {
      Ok(None) => ServiceState::Running,
      Ok(Some(_)) | Err(_) => {
        *child = None;
        ServiceState::Failed
      }
    }
  } else if executable.is_file() {
    ServiceState::Installed
  } else {
    ServiceState::NotInstalled
  }
}

fn mariadb_state(
  child: &mut Option<Child>,
  recovered_pid: &mut Option<u32>,
  executable: PathBuf,
) -> ServiceState {
  let state = child_state(child, executable);
  if matches!(state, ServiceState::Installed) && recovered_process_running(recovered_pid) {
    ServiceState::Running
  } else {
    state
  }
}

#[cfg(unix)]
fn recover_mariadb_pid(paths: &AppPaths, runtimes: &RuntimePaths) -> Option<u32> {
  use std::os::unix::fs::FileTypeExt;
  use std::os::unix::net::UnixStream;

  if !mariadb_server_binary(&runtimes.mariadb).is_file() {
    return None;
  }
  let service = paths.services.join("mariadb");
  let socket = service.join("mariadb.sock");
  if !std::fs::symlink_metadata(&socket)
    .ok()?
    .file_type()
    .is_socket()
  {
    return None;
  }
  let pid = std::fs::read_to_string(service.join("mariadb.pid"))
    .ok()?
    .trim()
    .parse::<u32>()
    .ok()?;
  if !unix_process_running(pid) || UnixStream::connect(socket).is_err() {
    return None;
  }
  Some(pid)
}

#[cfg(not(unix))]
fn recover_mariadb_pid(_paths: &AppPaths, _runtimes: &RuntimePaths) -> Option<u32> {
  None
}

#[cfg(unix)]
fn recovered_process_running(pid: &mut Option<u32>) -> bool {
  if pid.is_some_and(unix_process_running) {
    true
  } else {
    *pid = None;
    false
  }
}

#[cfg(not(unix))]
fn recovered_process_running(pid: &mut Option<u32>) -> bool {
  *pid = None;
  false
}

#[cfg(unix)]
fn unix_process_running(pid: u32) -> bool {
  use nix::sys::signal::kill;
  use nix::unistd::Pid;

  i32::try_from(pid)
    .ok()
    .is_some_and(|pid| kill(Pid::from_raw(pid), None).is_ok())
}

#[cfg(unix)]
async fn stop_process(pid: u32, description: &str) -> Result<()> {
  use nix::errno::Errno;
  use nix::sys::signal::{kill, Signal};
  use nix::unistd::Pid;

  let pid = i32::try_from(pid)
    .with_context(|| format!("{description} PID is outside the supported range"))?;
  let pid = Pid::from_raw(pid);
  if let Err(error) = kill(pid, Signal::SIGTERM) {
    if error != Errno::ESRCH {
      return Err(error).with_context(|| format!("unable to stop {description} process"));
    }
    return Ok(());
  }

  let started = tokio::time::Instant::now();
  while started.elapsed() < Duration::from_secs(3) {
    if !unix_process_running(pid.as_raw() as u32) {
      return Ok(());
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  if let Err(error) = kill(pid, Signal::SIGKILL) {
    if error != Errno::ESRCH {
      return Err(error).with_context(|| format!("unable to force-stop {description} process"));
    }
  }
  Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct RawPhpFpmPoolStatus {
  #[serde(rename = "active processes")]
  active_processes: u32,
  #[serde(rename = "idle processes")]
  idle_processes: u32,
  #[serde(rename = "total processes")]
  total_processes: u32,
  #[serde(rename = "listen queue")]
  listen_queue: u32,
  #[serde(rename = "max listen queue")]
  max_listen_queue: u32,
  #[serde(rename = "max children reached")]
  max_children_reached: u64,
  #[serde(rename = "slow requests")]
  slow_requests: u64,
}

async fn query_php_fpm_status(
  http_port: u16,
  domain: &str,
  version: &PhpVersion,
) -> Result<PhpFpmPoolStatus> {
  let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, http_port);
  let mut stream = tokio::time::timeout(
    Duration::from_millis(500),
    tokio::net::TcpStream::connect(address),
  )
  .await
  .context("PHP-FPM status connection timed out")??;
  let request = format!(
    "GET {PHP_FPM_STATUS_PATH}?json HTTP/1.0\r\nHost: {domain}\r\nConnection: close\r\n\r\n"
  );
  tokio::time::timeout(
    Duration::from_millis(500),
    stream.write_all(request.as_bytes()),
  )
  .await
  .context("PHP-FPM status request timed out")??;
  let mut response = Vec::new();
  tokio::time::timeout(
    Duration::from_millis(500),
    stream.read_to_end(&mut response),
  )
  .await
  .context("PHP-FPM status response timed out")??;
  let response = String::from_utf8(response).context("PHP-FPM status response is not UTF-8")?;
  parse_php_fpm_status_response(&response, version)
}

fn parse_php_fpm_status_response(response: &str, version: &PhpVersion) -> Result<PhpFpmPoolStatus> {
  let (headers, body) = response
    .split_once("\r\n\r\n")
    .context("PHP-FPM status response is missing HTTP headers")?;
  let status_line = headers.lines().next().unwrap_or_default();
  if !status_line.contains(" 200 ") {
    bail!("PHP-FPM status returned {status_line}");
  }
  let raw: RawPhpFpmPoolStatus =
    serde_json::from_str(body.trim()).context("invalid PHP-FPM status JSON")?;
  Ok(PhpFpmPoolStatus {
    version: version.clone(),
    active_processes: raw.active_processes,
    idle_processes: raw.idle_processes,
    total_processes: raw.total_processes,
    listen_queue: raw.listen_queue,
    max_listen_queue: raw.max_listen_queue,
    max_children_reached: raw.max_children_reached,
    slow_requests: raw.slow_requests,
  })
}

fn php_fpm_state(
  processes: &mut HashMap<PhpVersion, Child>,
  expected_versions: &BTreeSet<PhpVersion>,
  runtimes: &RuntimePaths,
) -> ServiceState {
  if expected_versions.is_empty() {
    return if runtimes.has_any_php() {
      ServiceState::Installed
    } else {
      ServiceState::NotInstalled
    };
  }

  let mut failed_versions = Vec::new();
  for version in expected_versions {
    match processes.get_mut(version) {
      Some(process) => match process.try_wait() {
        Ok(None) => {}
        Ok(Some(_)) | Err(_) => failed_versions.push(version.clone()),
      },
      None => failed_versions.push(version.clone()),
    }
  }
  if failed_versions.is_empty() {
    ServiceState::Running
  } else {
    for version in failed_versions {
      processes.remove(&version);
    }
    ServiceState::Failed
  }
}

fn dns_ingress_ready(port: u16) -> bool {
  let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
  let Ok(socket) = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)) else {
    return false;
  };
  if socket
    .set_read_timeout(Some(Duration::from_millis(200)))
    .is_err()
  {
    return false;
  }

  let query = [
    0xFA, 0xBD, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, b'f', b'a', b'b',
    b'd', b'e', b'v', 0x04, b't', b'e', b's', b't', 0x00, 0x00, 0x01, 0x00, 0x01,
  ];
  if socket.send_to(&query, address).is_err() {
    return false;
  }

  let mut response = [0u8; 512];
  match socket.recv_from(&mut response) {
    Ok((length, _)) => dns_response_is_success(&response[..length], [0xFA, 0xBD]),
    Err(_) => false,
  }
}

fn dns_response_is_success(response: &[u8], transaction_id: [u8; 2]) -> bool {
  response.len() >= 12
    && response[0..2] == transaction_id
    && response[2] & 0x80 != 0
    && response[3] & 0x0F == 0
    && u16::from_be_bytes([response[6], response[7]]) > 0
}

fn http_ingress_ready(port: u16) -> bool {
  TcpStream::connect_timeout(
    &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
    Duration::from_millis(200),
  )
  .is_ok()
}

async fn wait_for_ingress(ports: ServicePorts, timeout: Duration) -> Result<()> {
  let started = tokio::time::Instant::now();
  while started.elapsed() < timeout {
    if dns_ingress_ready(ports.dns)
      && http_ingress_ready(ports.http)
      && http_ingress_ready(ports.https)
    {
      return Ok(());
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  bail!(
    "system ingress is unavailable on DNS port {}, HTTP port {}, or HTTPS port {}",
    ports.dns,
    ports.http,
    ports.https
  )
}

#[cfg(test)]
mod tests {
  use std::cell::Cell;

  use fabdev_core::PhpVersion;
  use uuid::Uuid;

  use super::*;

  fn add_php_runtime(root: &Path, version: &str) -> PathBuf {
    let runtime = root.join(version);
    if cfg!(windows) {
      std::fs::create_dir_all(&runtime).expect("create PHP Runtime fixture");
      std::fs::write(runtime.join("php-cgi.exe"), "fixture").expect("write PHP CGI fixture");
    } else {
      std::fs::create_dir_all(runtime.join("sbin")).expect("create PHP Runtime fixture");
      std::fs::create_dir_all(runtime.join("lib/php/extensions/no-debug-non-zts-fixture"))
        .expect("create PHP extension fixture");
      std::fs::write(runtime.join("sbin/php-fpm"), "fixture").expect("write PHP-FPM fixture");
      std::fs::write(
        runtime.join("lib/php/extensions/no-debug-non-zts-fixture/opcache.so"),
        "fixture",
      )
      .expect("write OPcache fixture");
    }
    runtime
  }

  #[test]
  fn creates_windows_nginx_work_directories_idempotently() {
    let root = std::env::temp_dir().join(format!("fabdev-nginx-work-{}", Uuid::new_v4()));
    let runtime = root.join("nginx/current");

    ensure_windows_nginx_work_directories(&runtime).expect("create Nginx work directories");
    ensure_windows_nginx_work_directories(&runtime).expect("reuse Nginx work directories");

    assert!(runtime.join("logs").is_dir());
    assert!(runtime.join("temp").is_dir());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn identifies_only_managed_windows_runtime_processes() {
    let runtimes = RuntimePaths {
      dnsmasq: PathBuf::from(r"C:\FabDev\data\runtimes\dnsmasq\current"),
      nginx: PathBuf::from(r"C:\FabDev\data\runtimes\nginx\current"),
      php: PathBuf::from(r"C:\FabDev\data\runtimes\php"),
      mariadb: PathBuf::from(r"C:\FabDev\data\runtimes\mariadb\current"),
    };

    assert!(is_managed_windows_web_executable(
      Path::new(r"\\?\C:\FABDEV\data\runtimes\nginx\current\nginx.exe"),
      &runtimes
    ));
    assert!(is_managed_windows_web_executable(
      Path::new(r"C:\FabDev\data\runtimes\php\8.2.33\php-cgi.exe"),
      &runtimes
    ));
    assert!(!is_managed_windows_web_executable(
      Path::new(r"C:\tools\nginx\nginx.exe"),
      &runtimes
    ));
    assert!(is_managed_windows_mariadb_executable(
      Path::new(r"C:\FabDev\data\runtimes\mariadb\11.8.3\bin\mariadbd.exe"),
      &runtimes
    ));
    assert!(!is_managed_windows_mariadb_executable(
      Path::new(r"C:\MariaDB\bin\mariadbd.exe"),
      &runtimes
    ));
  }

  #[cfg(windows)]
  #[test]
  fn generate_configs_recreates_missing_windows_nginx_work_directories() {
    let root = std::env::temp_dir().join(format!("fabdev-nginx-config-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths::from_runtime_root(paths.runtimes.clone());
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project fixture");
    std::fs::create_dir_all(&runtimes.nginx).expect("create Nginx Runtime fixture");
    let site = Site {
      id: Uuid::new_v4(),
      name: "Windows Demo".to_owned(),
      domain: "windows-demo.test".to_owned(),
      project_path: project.clone(),
      document_root: project,
      php_version: None,
      enabled: true,
      secured: false,
    };

    generate_configs(
      &paths,
      &runtimes,
      ServicePorts {
        dns: 53,
        http: 80,
        https: 443,
        mariadb: 3306,
      },
      &[site],
    )
    .expect("generate Windows service configs");

    assert!(runtimes.nginx.join("logs").is_dir());
    assert!(runtimes.nginx.join("temp").is_dir());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn identifies_only_managed_web_processes_after_agent_restart() {
    let paths = AppPaths::from_root("/tmp/fabDev Application Support");
    let runtimes = RuntimePaths::from_runtime_root(paths.runtimes.clone());
    let php_74_config = paths.services.join("php/7.4/php-fpm.conf");
    let php_82_config = paths.services.join("php/8.2/php-fpm.conf");
    let processes = format!(
      "  100 {} --keep-in-foreground --conf-file={}\n  101 {} -p {}/ -c {} -g daemon off;\n  102 {}/8.2.33/sbin/php-fpm -y {} -F\n  103 php-fpm: master process ({})\n  104 /usr/local/sbin/dnsmasq --conf-file={}\n  105 {} --conf-file=/tmp/unmanaged-dnsmasq.conf\n  106 /Applications/fabDev.app/Contents/MacOS/fabdev-agent\n",
      dnsmasq_binary(&runtimes.dnsmasq).display(),
      paths.services.join("dnsmasq.conf").display(),
      nginx_binary(&runtimes.nginx).display(),
      runtimes.nginx.display(),
      paths.services.join("nginx/nginx.conf").display(),
      runtimes.php.display(),
      php_82_config.display(),
      php_74_config.display(),
      paths.services.join("dnsmasq.conf").display(),
      dnsmasq_binary(&runtimes.dnsmasq).display(),
    );

    let pids = managed_process_ids_from_output(&processes, 999, |command| {
      is_managed_web_process(command, &paths, &runtimes)
    });

    assert_eq!(pids, vec![100, 101, 102, 103]);
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn stop_all_terminates_an_untracked_managed_dns_process() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let root = std::env::temp_dir().join(format!("fabdev-orphan-dns-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths::from_runtime_root(paths.runtimes.clone());
    paths.ensure().expect("create app paths");
    std::fs::create_dir_all(runtimes.dnsmasq.join("sbin")).expect("create dnsmasq Runtime fixture");
    let dnsmasq = dnsmasq_binary(&runtimes.dnsmasq);
    std::fs::copy("/usr/bin/yes", &dnsmasq).expect("copy process fixture");
    let config = paths.services.join("dnsmasq.conf");
    std::fs::write(&config, "fixture").expect("write dnsmasq config fixture");
    let mut child = std::process::Command::new(&dnsmasq)
      .arg("--keep-in-foreground")
      .arg(format!("--conf-file={}", config.display()))
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("start untracked dnsmasq fixture");
    let pid = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
      let result = child.wait();
      let _ = sender.send(result);
    });

    let mut supervisor = ServiceSupervisor::new(
      paths,
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    let stopped = supervisor.stop_all().await;
    if stopped.is_err() {
      let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    let wait_result = receiver
      .recv_timeout(Duration::from_secs(2))
      .expect("untracked dnsmasq fixture exited");

    stopped.expect("stop untracked dnsmasq");
    wait_result.expect("reap untracked dnsmasq fixture");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn removes_stale_web_pid_and_socket_files_for_every_php_series() {
    let root = std::env::temp_dir().join(format!("fabdev-stale-services-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    for path in [
      paths.services.join("dnsmasq.pid"),
      paths.services.join("nginx/nginx.pid"),
      paths.services.join("php/7.4/php-fpm.pid"),
      paths.services.join("php/7.4/php-fpm.sock"),
      paths.services.join("php/8.2/php-fpm.pid"),
      paths.services.join("php/8.2/php-fpm.sock"),
    ] {
      std::fs::create_dir_all(path.parent().expect("service artifact parent"))
        .expect("create service artifact parent");
      std::fs::write(path, "fixture").expect("write service artifact");
    }

    remove_web_service_artifacts(&paths).expect("remove stale service artifacts");

    assert!(!paths.services.join("dnsmasq.pid").exists());
    assert!(!paths.services.join("nginx/nginx.pid").exists());
    assert!(!paths.services.join("php/7.4/php-fpm.pid").exists());
    assert!(!paths.services.join("php/7.4/php-fpm.sock").exists());
    assert!(!paths.services.join("php/8.2/php-fpm.pid").exists());
    assert!(!paths.services.join("php/8.2/php-fpm.sock").exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn parses_php_fpm_status_metrics() {
    let response = concat!(
      "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n",
      r#"{"active processes":3,"idle processes":5,"total processes":8,"listen queue":1,"max listen queue":2,"max children reached":4,"slow requests":6}"#
    );

    let version = "8.2".parse::<PhpVersion>().expect("parse PHP version");
    let status =
      parse_php_fpm_status_response(response, &version).expect("parse PHP-FPM status response");

    assert_eq!(status.version, version);
    assert_eq!(status.active_processes, 3);
    assert_eq!(status.idle_processes, 5);
    assert_eq!(status.total_processes, 8);
    assert_eq!(status.listen_queue, 1);
    assert_eq!(status.max_listen_queue, 2);
    assert_eq!(status.max_children_reached, 4);
    assert_eq!(status.slow_requests, 6);
  }

  #[test]
  fn rotates_managed_logs_with_bounded_retention() {
    let root = std::env::temp_dir().join(format!("fabdev-log-rotation-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create log fixture directory");
    let log = root.join("nginx-access.log");

    std::fs::write(&log, "first").expect("write first log fixture");
    assert!(rotate_log_file(&log, 1, 2).expect("rotate first log fixture"));
    assert_eq!(
      std::fs::read_to_string(&log).expect("read truncated log"),
      ""
    );
    assert_eq!(
      std::fs::read_to_string(rotated_log_path(&log, 1)).expect("read first archive"),
      "first"
    );

    std::fs::write(&log, "second").expect("write second log fixture");
    assert!(rotate_log_file(&log, 1, 2).expect("rotate second log fixture"));
    assert_eq!(
      std::fs::read_to_string(rotated_log_path(&log, 1)).expect("read latest archive"),
      "second"
    );
    assert_eq!(
      std::fs::read_to_string(rotated_log_path(&log, 2)).expect("read retained archive"),
      "first"
    );

    std::fs::write(&log, "third").expect("write third log fixture");
    assert!(rotate_log_file(&log, 1, 2).expect("rotate third log fixture"));
    assert_eq!(
      std::fs::read_to_string(rotated_log_path(&log, 1)).expect("read newest archive"),
      "third"
    );
    assert_eq!(
      std::fs::read_to_string(rotated_log_path(&log, 2)).expect("read previous archive"),
      "second"
    );

    std::fs::remove_dir_all(root).expect("remove log fixture directory");
  }

  #[test]
  fn generates_isolated_mariadb_config() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-config-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtime = root.join("runtime/mariadb/12.3.2");

    let generated =
      generate_mariadb_config(&paths, &runtime, 3306).expect("generate MariaDB config");
    let contents = std::fs::read_to_string(&generated.config).expect("read MariaDB config");

    assert!(contents.contains("bind-address = 127.0.0.1"));
    assert!(contents.contains("port = 3306"));
    assert!(contents.contains(&format!(
      "# User configuration ({})\n[mariadbd]",
      mariadb_config_filename()
    )));
    assert!(!contents.contains("max_connections"));
    assert!(contents.contains(&format!(
      "basedir = \"{}\"",
      escape_mariadb_quoted_value(&runtime.to_string_lossy())
    )));
    assert!(contents.contains(&format!(
      "datadir = \"{}\"",
      escape_mariadb_quoted_value(&generated.data.to_string_lossy())
    )));
    assert!(!contents.contains("D:/mysql_data"));
    assert!(!contents.contains("innodb_thread_concurrency"));
    assert!(!contents.contains("innodb_flush_method"));
    assert!(!contents.contains("innodb_file_per_table"));
    assert!(contents.contains(&format!(
      "pid-file = \"{}\"",
      escape_mariadb_quoted_value(&generated.pid.to_string_lossy())
    )));
    assert!(!contents.contains("/opt/homebrew"));
    if cfg!(windows) {
      assert!(contents.contains("skip-ssl"));
    } else {
      assert!(!contents.contains("skip-ssl"));
    }
    assert!(generated.data.is_dir());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn merges_safe_custom_mariadb_options_before_managed_options() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-custom-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtime = root.join("runtime/mariadb/12.3.2");
    let custom_path = ensure_mariadb_custom_config(&paths).expect("create custom config");
    std::fs::write(
      custom_path,
      "[mariadbd]\nmax_connections = 250\ncharacter-set-server = utf8mb4\n",
    )
    .expect("write custom config");

    let generated =
      generate_mariadb_config(&paths, &runtime, 3306).expect("generate MariaDB config");
    let contents = std::fs::read_to_string(generated.config).expect("read MariaDB config");

    let custom_position = contents
      .find("max_connections = 250")
      .expect("custom option");
    let managed_position = contents
      .find("bind-address = 127.0.0.1")
      .expect("managed option");
    assert!(custom_position < managed_position);
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn creates_empty_mariadb_custom_config() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-empty-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let custom_path = ensure_mariadb_custom_config(&paths).expect("create custom config");
    let contents = std::fs::read_to_string(custom_path).expect("read custom config");

    assert_eq!(contents, "[mariadbd]\n\n");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rejects_managed_or_included_mariadb_options() {
    let managed =
      validate_mariadb_custom_config("[mariadbd]\nport = 3307\n").expect_err("reject managed port");
    assert!(managed.to_string().contains("fabDev-managed option: port"));

    let included = validate_mariadb_custom_config("[mariadbd]\n!includedir /tmp/options\n")
      .expect_err("reject included directory");
    assert!(included.to_string().contains("cannot include another file"));
  }

  #[cfg(unix)]
  #[test]
  fn validates_before_saving_custom_mariadb_config() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("fabdev-mariadb-validate-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths::from_runtime_root(root.join("runtime"));
    std::fs::create_dir_all(runtimes.mariadb.join("bin")).expect("create MariaDB bin fixture");
    let server = mariadb_server_binary(&runtimes.mariadb);
    std::fs::write(&server, "#!/bin/sh\nexit 0\n").expect("write MariaDB fixture");
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755))
      .expect("make MariaDB fixture executable");
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    let saved = supervisor
      .save_mariadb_config("[mariadbd]\nmax_connections = 250\n")
      .expect("save validated config");

    assert_eq!(saved.0, mariadb_config_filename());
    assert_eq!(
      std::fs::read_to_string(mariadb_custom_config_path(&paths)).expect("read saved config"),
      saved.1
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn escapes_mariadb_password_as_a_supported_string_literal() {
    let password = "p'a\\ss密碼";
    let statement = mariadb_password_statement(password);

    assert_eq!(
      statement,
      "SET SESSION sql_mode = 'NO_BACKSLASH_ESCAPES';\nALTER USER 'root'@'127.0.0.1' IDENTIFIED BY 'p''a\\ss密碼', 'root'@'localhost' IDENTIFIED BY 'p''a\\ss密碼';\n"
    );
  }

  #[test]
  fn removes_password_and_sql_text_from_mariadb_errors() {
    let syntax_error = b"--------------\nSET PASSWORD = PASSWORD('secret')\n--------------\nERROR 1064 (42000) at line 1: syntax error near 'secret'";
    let authentication_error = b"ERROR 1045 (28000): Access denied for user 'root'@'localhost'";

    let sanitized = mariadb_password_error(syntax_error);
    assert_eq!(
      sanitized,
      "MariaDB rejected the root password change (error 1064)"
    );
    assert!(!sanitized.contains("secret"));
    assert_eq!(
      mariadb_password_error(authentication_error),
      "unable to authenticate as MariaDB root; verify the current password"
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn sends_mariadb_password_through_stdin_without_command_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("fabdev-mariadb-password-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths::from_runtime_root(root.join("runtime"));
    std::fs::create_dir_all(runtimes.mariadb.join("bin")).expect("create MariaDB bin fixture");
    let client = mariadb_client_binary(&runtimes.mariadb);
    let arguments = root.join("arguments.txt");
    let environment = root.join("environment.txt");
    let input = root.join("input.txt");
    std::fs::write(
      &client,
      format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf '%s' \"$MYSQL_PWD\" > {}\ncat > {}\n",
        arguments.display(),
        environment.display(),
        input.display()
      ),
    )
    .expect("write MariaDB client fixture");
    std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755))
      .expect("make MariaDB client fixture executable");
    let mut supervisor = ServiceSupervisor::new(
      paths,
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor.mariadb = Some(
      Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("start running service fixture"),
    );

    supervisor
      .set_mariadb_root_password("current secret", "new secret")
      .await
      .expect("change root password");

    let captured_arguments = std::fs::read_to_string(arguments).expect("read client arguments");
    assert!(captured_arguments.contains("--protocol=tcp"));
    assert!(captured_arguments.contains("--host=127.0.0.1"));
    assert!(captured_arguments.contains("--port=3306"));
    assert!(!captured_arguments.contains("current secret"));
    assert!(!captured_arguments.contains("new secret"));
    assert_eq!(
      std::fs::read_to_string(environment).expect("read client environment"),
      "current secret"
    );
    assert_eq!(
      std::fs::read_to_string(input).expect("read client input"),
      mariadb_password_statement("new secret")
    );
    supervisor
      .stop_mariadb()
      .await
      .expect("stop service fixture");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[tokio::test]
  #[ignore = "requires FABDEV_MARIADB_TEST_RUNTIME with an installed MariaDB Runtime"]
  async fn synchronizes_tcp_and_socket_root_passwords_with_installed_runtime() {
    let runtime = PathBuf::from(
      std::env::var("FABDEV_MARIADB_TEST_RUNTIME")
        .expect("set FABDEV_MARIADB_TEST_RUNTIME to an installed MariaDB Runtime"),
    );
    let root = PathBuf::from("/tmp").join(format!("fabdev-mdb-auth-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let data_dir = paths.services.join("mariadb/data");
    std::fs::create_dir_all(&data_dir).expect("create MariaDB data fixture");
    let port_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve test port");
    let port = port_listener.local_addr().expect("read test port").port();
    drop(port_listener);
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtime/dnsmasq/current"),
      nginx: root.join("runtime/nginx/current"),
      php: root.join("runtime/php"),
      mariadb: runtime.clone(),
    };
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes.clone(),
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: port,
      },
    );
    supervisor
      .save_mariadb_settings(MariaDbSettings {
        port,
        data_dir,
        connection_mode: MariaDbConnectionMode::Managed,
        system_socket: default_mariadb_system_socket(),
      })
      .expect("save isolated MariaDB settings");
    supervisor
      .start_mariadb_and_remember()
      .await
      .expect("start isolated MariaDB");

    let password = "fixture'p\\ass密碼";
    supervisor
      .set_mariadb_root_password("", password)
      .await
      .expect("synchronize local root passwords");

    supervisor
      .stop_mariadb()
      .await
      .expect("simulate Agent shutdown");
    drop(supervisor);

    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: port,
      },
    );
    supervisor
      .restore_mariadb_last_state()
      .await
      .expect("restore remembered MariaDB state");
    let client = mariadb_client_binary(&runtime);
    let tcp = std::process::Command::new(&client)
      .args([
        "--no-defaults".to_owned(),
        "--protocol=tcp".to_owned(),
        "--host=127.0.0.1".to_owned(),
        format!("--port={port}"),
        "--user=root".to_owned(),
        "--execute=SELECT CURRENT_USER()".to_owned(),
      ])
      .env("MYSQL_PWD", password)
      .output()
      .expect("verify restored TCP root login");
    let socket = std::process::Command::new(&client)
      .args([
        "--no-defaults".to_owned(),
        "--protocol=socket".to_owned(),
        format!(
          "--socket={}",
          paths.services.join("mariadb/mariadb.sock").display()
        ),
        "--user=root".to_owned(),
        "--execute=SELECT CURRENT_USER()".to_owned(),
      ])
      .env("MYSQL_PWD", password)
      .output()
      .expect("verify restored Socket root login");
    assert!(
      tcp.status.success(),
      "TCP root login failed: {}",
      String::from_utf8_lossy(&tcp.stderr)
    );
    assert!(
      socket.status.success(),
      "Socket root login failed: {}",
      String::from_utf8_lossy(&socket.stderr)
    );
    assert_eq!(supervisor.status().mariadb, ServiceState::Running);
    supervisor
      .stop_mariadb_and_remember()
      .await
      .expect("stop restored MariaDB");
    assert!(!load_mariadb_desired_state(&paths).expect("load final stopped state"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn saves_and_applies_mariadb_console_settings() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-settings-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let database = root.join("custom database");
    std::fs::create_dir_all(&database).expect("create custom database directory");
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtime/dnsmasq/current"),
      nginx: root.join("runtime/nginx/current"),
      php: root.join("runtime/php"),
      mariadb: root.join("runtime/mariadb/current"),
    };
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes.clone(),
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    let saved = supervisor
      .save_mariadb_settings(MariaDbSettings {
        port: 3307,
        data_dir: database.clone(),
        connection_mode: MariaDbConnectionMode::Managed,
        system_socket: default_mariadb_system_socket(),
      })
      .expect("save MariaDB settings");
    assert_eq!(saved.port, 3307);
    assert_eq!(
      saved.data_dir,
      database.canonicalize().expect("resolve database")
    );
    assert_eq!(supervisor.mariadb_settings().expect("load settings"), saved);

    let generated = generate_mariadb_config_with_settings(&paths, &runtimes.mariadb, &saved)
      .expect("generate configured MariaDB settings");
    let contents = std::fs::read_to_string(generated.config).expect("read MariaDB config");
    assert!(contents.contains("port = 3307"));
    assert!(contents.contains(&format!(
      "datadir = \"{}\"",
      escape_mariadb_quoted_value(&saved.data_dir.to_string_lossy())
    )));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn restores_a_missing_default_mariadb_data_directory() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-restore-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let data_dir = default_mariadb_data_dir(&paths);
    std::fs::create_dir_all(&data_dir).expect("create default MariaDB data directory");
    save_mariadb_settings_file(
      &paths,
      &MariaDbSettings {
        port: 3306,
        data_dir: data_dir.clone(),
        connection_mode: MariaDbConnectionMode::Managed,
        system_socket: default_mariadb_system_socket(),
      },
    )
    .expect("save default MariaDB settings");
    std::fs::remove_dir_all(paths.services.join("mariadb"))
      .expect("simulate deleting the MariaDB service directory");

    let loaded = load_mariadb_settings(&paths, 3306)
      .expect("restore the missing default MariaDB data directory");

    assert!(data_dir.is_dir());
    assert_eq!(
      loaded.data_dir,
      data_dir
        .canonicalize()
        .expect("resolve restored data directory")
    );
    assert!(std::fs::read_dir(&data_dir)
      .expect("read restored data directory")
      .next()
      .is_none());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn does_not_restore_a_missing_custom_mariadb_data_directory() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-custom-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let custom_data_dir = root.join("external/mariadb-data");
    std::fs::create_dir_all(&custom_data_dir).expect("create custom MariaDB data directory");
    save_mariadb_settings_file(
      &paths,
      &MariaDbSettings {
        port: 3306,
        data_dir: custom_data_dir.clone(),
        connection_mode: MariaDbConnectionMode::Managed,
        system_socket: default_mariadb_system_socket(),
      },
    )
    .expect("save custom MariaDB settings");
    std::fs::remove_dir_all(&custom_data_dir)
      .expect("simulate an unavailable custom MariaDB data directory");

    let error = load_mariadb_settings(&paths, 3306)
      .expect_err("reject the missing custom MariaDB data directory");

    assert!(error
      .to_string()
      .contains("MariaDB data directory does not exist"));
    assert!(!custom_data_dir.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn removes_only_a_known_incomplete_windows_mariadb_initialization_file() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-partial-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let data_dir = default_mariadb_data_dir(&paths);
    std::fs::create_dir_all(&data_dir).expect("create default MariaDB data directory");
    let partial = data_dir.join("my.ini");
    std::fs::write(
      &partial,
      format!(
        "[mysqld]\r\ndatadir={}\r\n[client]\r\nplugin-dir={}/lib/plugin\r\n",
        data_dir.display(),
        paths.runtimes.join("mariadb/12.3.2").display()
      ),
    )
    .expect("write incomplete MariaDB initialization file");
    let settings = default_mariadb_settings(&paths, 3306);

    restore_incomplete_default_mariadb_initialization(&paths, &settings, true)
      .expect("remove known incomplete MariaDB initialization file");

    assert!(!partial.exists());
    assert!(std::fs::read_dir(&data_dir)
      .expect("read recovered MariaDB data directory")
      .next()
      .is_none());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn preserves_unknown_or_nonexclusive_mariadb_data_directory_contents() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-preserve-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let data_dir = default_mariadb_data_dir(&paths);
    std::fs::create_dir_all(&data_dir).expect("create default MariaDB data directory");
    let partial = data_dir.join("my.ini");
    let generated_stub = format!(
      "[mysqld]\ndatadir={}\n[client]\nplugin-dir={}/lib/plugin\n",
      data_dir.display(),
      paths.runtimes.join("mariadb/12.3.2").display()
    );
    std::fs::write(&partial, &generated_stub).expect("write MariaDB initialization file");
    std::fs::write(data_dir.join("keep.txt"), "user data").expect("write unrelated user file");
    let settings = default_mariadb_settings(&paths, 3306);

    restore_incomplete_default_mariadb_initialization(&paths, &settings, true)
      .expect("preserve nonexclusive data directory");
    assert!(partial.is_file());

    std::fs::remove_file(data_dir.join("keep.txt")).expect("remove unrelated fixture");
    std::fs::write(&partial, "[mysqld]\nport=3307\n").expect("write custom MariaDB config");
    restore_incomplete_default_mariadb_initialization(&paths, &settings, true)
      .expect("preserve unknown my.ini");
    assert_eq!(
      std::fs::read_to_string(&partial).expect("read preserved custom config"),
      "[mysqld]\nport=3307\n"
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn recognizes_the_windows_mariadb_installer_stub_from_the_failed_candidate() {
    let paths = AppPaths::from_root(PathBuf::from(
      r"C:\Users\jimmywon\AppData\Local\fabDev\data",
    ));
    let contents = "[mysqld]\n\
datadir=//?/C:/Users/jimmywon/AppData/Local/fabDev/data/services/mariadb/data\n\
[client]\n\
plugin-dir=C:\\Users\\jimmywon\\AppData\\Local\\FabDev\\data\\runtimes\\mariadb\\12.3.2/lib/plugin\n";

    assert!(is_windows_mariadb_install_db_stub(
      &paths,
      &default_mariadb_data_dir(&paths),
      contents
    ));
  }

  #[test]
  fn passes_a_normal_windows_path_to_the_mariadb_installer() {
    assert_eq!(
      mariadb_install_data_dir_argument(
        Path::new(r"\\?\C:\Users\jimmywon\AppData\Local\fabDev\data\services\mariadb\data"),
        true
      ),
      r"--datadir=C:\Users\jimmywon\AppData\Local\fabDev\data\services\mariadb\data"
    );
  }

  #[test]
  fn removes_windows_verbatim_prefixes_from_user_visible_paths() {
    assert_eq!(
      windows_path_without_verbatim_prefix(Path::new(
        r"\\?\C:\Users\jimmywon\AppData\Local\fabDev\data"
      )),
      r"C:\Users\jimmywon\AppData\Local\fabDev\data"
    );
    assert_eq!(
      windows_path_without_verbatim_prefix(Path::new(
        "//?/C:/Users/jimmywon/AppData/Local/fabDev/data"
      )),
      "C:/Users/jimmywon/AppData/Local/fabDev/data"
    );
    assert_eq!(
      windows_path_without_verbatim_prefix(Path::new(r"\\?\UNC\server\share\fabDev")),
      r"\\server\share\fabDev"
    );
  }

  #[test]
  fn persists_mariadb_desired_state_independently() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-state-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));

    assert!(!load_mariadb_desired_state(&paths).expect("load default state"));
    save_mariadb_desired_state(&paths, true).expect("remember running state");
    assert!(load_mariadb_desired_state(&paths).expect("load running state"));
    save_mariadb_desired_state(&paths, false).expect("remember stopped state");
    assert!(!load_mariadb_desired_state(&paths).expect("load stopped state"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn leaves_mariadb_stopped_without_a_running_desired_state() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-restore-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths::from_runtime_root(root.join("runtime"));
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    supervisor
      .restore_mariadb_last_state()
      .await
      .expect("restore default stopped state");
    assert_eq!(supervisor.status().mariadb, ServiceState::NotInstalled);

    save_mariadb_desired_state(&paths, true).expect("remember running state");
    supervisor
      .restore_mariadb_last_state()
      .await
      .expect("skip unavailable Runtime");
    assert_eq!(supervisor.status().mariadb, ServiceState::NotInstalled);
    assert!(load_mariadb_desired_state(&paths).expect("preserve running state"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rejects_non_database_directory_for_mariadb_settings() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-invalid-{}", Uuid::new_v4()));
    let database = root.join("documents");
    std::fs::create_dir_all(&database).expect("create directory fixture");
    std::fs::write(database.join("report.txt"), "fixture").expect("write unrelated fixture");

    let error = validate_mariadb_settings(MariaDbSettings {
      port: 3306,
      data_dir: database,
      connection_mode: MariaDbConnectionMode::Managed,
      system_socket: default_mariadb_system_socket(),
    })
    .expect_err("reject unrelated non-empty directory");

    assert!(error
      .to_string()
      .contains("must be empty or contain an existing MariaDB database"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn runs_mariadb_installer_relative_to_runtime_with_spaces() {
    let runtime = PathBuf::from("/tmp/fabDev Application Support/runtimes/mariadb/current");
    let installer = runtime.join("scripts/mariadb-install-db");
    let command = mariadb_install_command(&runtime, &installer).expect("build install command");
    let command = command.as_std();

    assert_eq!(command.get_program(), "/bin/sh");
    assert_eq!(command.get_current_dir(), Some(runtime.as_path()));
    assert_eq!(
      command.get_args().collect::<Vec<_>>(),
      vec![std::ffi::OsStr::new("scripts/mariadb-install-db")]
    );
  }

  #[cfg(windows)]
  #[test]
  fn runs_mariadb_windows_installer_without_unix_options() {
    let runtime =
      PathBuf::from(r"C:\Users\fabdev\AppData\Local\FabDev\data\runtimes\mariadb\12.3.2");
    let installer = runtime.join("bin/mariadb-install-db.exe");
    let command = mariadb_install_command(&runtime, &installer).expect("build install command");
    let command = command.as_std();

    assert_eq!(command.get_program(), installer.as_os_str());
    assert_eq!(command.get_current_dir(), Some(runtime.as_path()));
    assert_eq!(command.get_args().count(), 0);

    let config = GeneratedMariaDbConfig {
      config: PathBuf::from(r"C:\FabDev\services\mariadb\my.ini"),
      data: PathBuf::from(r"C:\FabDev\services\mariadb\data"),
      pid: PathBuf::from(r"C:\FabDev\services\mariadb\mariadb.pid"),
      socket: PathBuf::from(r"C:\FabDev\services\mariadb\mariadb.sock"),
    };
    let args = mariadb_install_args(&config);

    assert_eq!(
      args,
      vec![
        r"--datadir=C:\FabDev\services\mariadb\data".to_owned(),
        "--silent".to_owned(),
      ]
    );
    assert!(!args.iter().any(|arg| {
      arg == "--no-defaults"
        || arg.starts_with("--basedir=")
        || arg.starts_with("--auth-root-authentication-method=")
        || arg == "--skip-name-resolve"
        || arg == "--skip-test-db"
    }));
  }

  #[cfg(unix)]
  #[test]
  fn recovers_running_mariadb_after_agent_restart() {
    use std::os::unix::net::UnixListener;

    let root = PathBuf::from("/tmp").join(format!("fmd-r-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtime/dnsmasq/current"),
      nginx: root.join("runtime/nginx/current"),
      php: root.join("runtime/php"),
      mariadb: root.join("runtime/mariadb/current"),
    };
    std::fs::create_dir_all(runtimes.mariadb.join("bin")).expect("create MariaDB Runtime fixture");
    std::fs::write(mariadb_server_binary(&runtimes.mariadb), "fixture")
      .expect("write MariaDB server fixture");
    let service = paths.services.join("mariadb");
    std::fs::create_dir_all(&service).expect("create MariaDB service fixture");
    std::fs::write(service.join("mariadb.pid"), std::process::id().to_string())
      .expect("write MariaDB PID fixture");
    let listener =
      UnixListener::bind(service.join("mariadb.sock")).expect("bind MariaDB socket fixture");

    let mut supervisor = ServiceSupervisor::new(
      paths,
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    assert_eq!(supervisor.status().mariadb, ServiceState::Running);
    assert_eq!(supervisor.recovered_mariadb_pid, Some(std::process::id()));
    drop(listener);
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn ignores_stale_mariadb_pid_after_agent_restart() {
    use std::os::unix::net::UnixListener;

    let root = PathBuf::from("/tmp").join(format!("fmd-s-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtime/dnsmasq/current"),
      nginx: root.join("runtime/nginx/current"),
      php: root.join("runtime/php"),
      mariadb: root.join("runtime/mariadb/current"),
    };
    std::fs::create_dir_all(runtimes.mariadb.join("bin")).expect("create MariaDB Runtime fixture");
    std::fs::write(mariadb_server_binary(&runtimes.mariadb), "fixture")
      .expect("write MariaDB server fixture");
    let service = paths.services.join("mariadb");
    std::fs::create_dir_all(&service).expect("create MariaDB service fixture");
    std::fs::write(service.join("mariadb.pid"), u32::MAX.to_string())
      .expect("write MariaDB PID fixture");
    let listener =
      UnixListener::bind(service.join("mariadb.sock")).expect("bind MariaDB socket fixture");

    let mut supervisor = ServiceSupervisor::new(
      paths,
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    assert_eq!(supervisor.status().mariadb, ServiceState::Installed);
    assert_eq!(supervisor.recovered_mariadb_pid, None);
    drop(listener);
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn reports_unavailable_mariadb_port() {
    let error = tcp_port_unavailable_error(
      std::io::Error::new(std::io::ErrorKind::AddrInUse, "fixture listener"),
      3306,
      "MariaDB",
    );

    assert_eq!(
      error.to_string(),
      "MariaDB cannot use 127.0.0.1:3306; the port is unavailable"
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn stopping_mariadb_keeps_web_services_running() {
    let root = std::env::temp_dir().join(format!("fabdev-mariadb-stop-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create app paths");
    let mut supervisor = ServiceSupervisor::new(
      paths,
      RuntimePaths {
        dnsmasq: root.join("runtime/dnsmasq/current"),
        nginx: root.join("runtime/nginx/current"),
        php: root.join("runtime/php"),
        mariadb: root.join("runtime/mariadb/current"),
      },
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor.nginx = Some(
      Command::new("sh")
        .args(["-c", "sleep 30"])
        .spawn()
        .expect("start web fixture"),
    );
    supervisor.mariadb = Some(
      Command::new("sh")
        .args(["-c", "sleep 30"])
        .spawn()
        .expect("start MariaDB fixture"),
    );

    let error = supervisor
      .save_mariadb_settings(MariaDbSettings {
        port: 3307,
        data_dir: root.join("database"),
        connection_mode: MariaDbConnectionMode::Managed,
        system_socket: default_mariadb_system_socket(),
      })
      .expect_err("reject settings while MariaDB is running");
    assert_eq!(
      error.to_string(),
      "stop MariaDB before changing its settings"
    );

    supervisor.stop_mariadb().await.expect("stop only MariaDB");

    assert!(supervisor.mariadb.is_none());
    assert!(supervisor
      .nginx
      .as_mut()
      .expect("web fixture remains tracked")
      .try_wait()
      .expect("read web fixture state")
      .is_none());
    supervisor.stop_all().await.expect("stop web fixture");
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn generates_isolated_service_configs() {
    let root = std::env::temp_dir().join(format!("fabdev-services-{}", Uuid::new_v4()));
    let project = root.join("project/public");
    std::fs::create_dir_all(&project).expect("create fixture");
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    add_php_runtime(&runtimes.php, "7.4.33");
    let mariadb_server = mariadb_server_binary(&runtimes.mariadb);
    std::fs::create_dir_all(mariadb_server.parent().expect("MariaDB binary parent"))
      .expect("create MariaDB Runtime fixture");
    std::fs::write(mariadb_server, "").expect("create MariaDB server fixture");
    let site = Site {
      id: Uuid::new_v4(),
      name: "ERP Demo".to_owned(),
      domain: "erp-demo.test".to_owned(),
      project_path: root.join("project"),
      document_root: project.clone(),
      php_version: Some(PhpVersion { major: 7, minor: 4 }),
      enabled: true,
      secured: true,
    };
    let second_site = Site {
      id: Uuid::new_v4(),
      name: "CRM Demo".to_owned(),
      domain: "crm-demo.test".to_owned(),
      project_path: root.join("project"),
      document_root: project,
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    };

    let generated = generate_configs(
      &paths,
      &runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
      &[site, second_site],
    )
    .expect("generate configs");
    let nginx_site =
      std::fs::read_to_string(paths.sites.join("erp-demo.test.conf")).expect("read site config");
    let second_nginx_site =
      std::fs::read_to_string(paths.sites.join("crm-demo.test.conf")).expect("read second config");
    let nginx_global =
      std::fs::read_to_string(paths.services.join("nginx/nginx.conf")).expect("read nginx config");
    #[cfg(unix)]
    let php_pool = std::fs::read_to_string(paths.services.join("php/8.2/php-fpm.d/www.conf"))
      .expect("read pool config");
    let php_74_ini =
      std::fs::read_to_string(paths.config.join("php/7.4/php.ini")).expect("read PHP 7.4 ini");
    let php_82_ini =
      std::fs::read_to_string(paths.config.join("php/8.2/php.ini")).expect("read PHP 8.2 ini");
    assert!(nginx_site.contains("listen 127.0.0.1:8080;"));
    assert!(nginx_site.contains("return 301 https://$host$request_uri;"));
    assert!(nginx_site.contains("listen 127.0.0.1:8443 ssl;"));
    assert!(nginx_site.contains("ssl_certificate"));
    #[cfg(unix)]
    assert!(nginx_site.contains("services/php/7.4/php-fpm.sock"));
    #[cfg(windows)]
    assert!(nginx_site.contains("fastcgi_pass 127.0.0.1:19074;"));
    assert!(second_nginx_site.contains("server_name crm-demo.test;"));
    #[cfg(unix)]
    assert!(second_nginx_site.contains("services/php/8.2/php-fpm.sock"));
    #[cfg(windows)]
    assert!(second_nginx_site.contains("fastcgi_pass 127.0.0.1:19082;"));
    assert!(nginx_global.contains("listen 127.0.0.1:8080 default_server;"));
    assert!(nginx_global.contains("listen 127.0.0.1:8443 ssl default_server;"));
    assert!(nginx_global.contains("server_names_hash_bucket_size 512;"));
    assert!(nginx_global.contains("log_format fabdev_timing"));
    assert!(nginx_global.contains("request_time=$request_time"));
    assert!(nginx_global.contains("upstream_response_time=$upstream_response_time"));
    assert!(nginx_global.contains("logs/nginx-access.log\" fabdev_timing;"));
    assert!(paths.config.join("tls/ca.crt").is_file());
    assert!(paths.config.join("tls/sites/erp-demo.test.crt").is_file());
    assert!(nginx_global.contains(&format!(
      "error_log \"{}/logs/nginx-error.log\" notice;",
      escape_nginx_quoted_value(&paths.root.to_string_lossy())
    )));
    assert!(nginx_global.contains(&format!(
      "include \"{}/sites/*.conf\";",
      escape_nginx_quoted_value(&paths.root.to_string_lossy())
    )));
    assert!(nginx_global.contains("add_header X-fabDev-Default 1 always;"));
    assert!(nginx_global.contains("return 404;"));
    #[cfg(unix)]
    {
      assert!(php_pool.contains("services/php/8.2/php-fpm.sock"));
      assert!(php_pool.contains("pm.max_children = 16"));
      assert!(php_pool.contains("pm.start_servers = 4"));
      assert!(php_pool.contains("pm.min_spare_servers = 2"));
      assert!(php_pool.contains("pm.max_spare_servers = 6"));
      assert!(php_pool.contains("pm.max_requests = 500"));
      assert!(php_pool.contains("pm.status_path = /__fabdev/php-fpm-status"));
      assert!(php_pool.contains("request_slowlog_timeout = 10s"));
      assert!(php_pool.contains("request_terminate_timeout = 120s"));
      assert!(php_pool.contains(&format!(
        "slowlog = {}/services/php/8.2/logs/php-slow.log",
        paths.root.display()
      )));
      let mariadb_socket = default_mariadb_system_socket();
      assert!(php_pool.contains(&format!(
        "php_admin_value[mysqli.default_socket] = {}",
        mariadb_socket.display()
      )));
      assert!(php_pool.contains(&format!(
        "php_admin_value[pdo_mysql.default_socket] = {}",
        mariadb_socket.display()
      )));
      assert!(!php_pool.contains("services/mariadb/mariadb.sock"));
    }
    for php_ini in [php_74_ini, php_82_ini] {
      assert!(php_ini.contains("post_max_size = 64M"));
      assert!(php_ini.contains("upload_max_filesize = 64M"));
      assert!(php_ini.contains("date.timezone = \"Asia/Taipei\""));
      assert!(!php_ini.contains("/Users/example"));
    }
    #[cfg(unix)]
    assert!(paths.services.join("php/7.4/php-fpm.d/www.conf").is_file());
    assert_eq!(generated.php.len(), 2);
    assert!(generated.dnsmasq.is_file());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn preserves_system_socket_across_legacy_settings_rewrite() {
    let fixture_id = Uuid::new_v4().simple().to_string();
    let root = std::env::temp_dir().join(format!("fdms-{}", &fixture_id[..8]));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let system_socket = root.join("homebrew/mysql.sock");
    #[cfg(unix)]
    std::fs::create_dir_all(system_socket.parent().expect("System Socket parent"))
      .expect("create System Socket directory");
    #[cfg(unix)]
    let _system_listener = std::os::unix::net::UnixListener::bind(&system_socket)
      .expect("bind selected System MariaDB Socket");
    let mut supervisor =
      ServiceSupervisor::new(paths.clone(), runtimes.clone(), ServicePorts::system());
    supervisor
      .save_mariadb_settings(MariaDbSettings {
        port: 3306,
        data_dir: root.join("unused-managed-data"),
        connection_mode: MariaDbConnectionMode::System,
        system_socket: system_socket.clone(),
      })
      .expect("save System MariaDB connection without managed data directory");
    let legacy_settings = serde_json::json!({
      "port": 3306,
      "dataDir": root.join("legacy-managed-data"),
    });
    std::fs::write(
      mariadb_settings_path(&paths),
      serde_json::to_vec_pretty(&legacy_settings).expect("serialize legacy MariaDB settings"),
    )
    .expect("simulate an older fabDev writing legacy MariaDB settings");

    let loaded = load_mariadb_settings(&paths, 3306)
      .expect("load separate System MariaDB connection settings");
    assert_eq!(loaded.connection_mode, MariaDbConnectionMode::System);
    assert_eq!(loaded.system_socket, system_socket);

    generate_php_config(
      &paths,
      &runtimes,
      &"8.2".parse().expect("parse PHP version"),
    )
    .expect("generate PHP config for System MariaDB");
    let php_pool = std::fs::read_to_string(paths.services.join("php/8.2/php-fpm.d/www.conf"))
      .expect("read PHP-FPM pool");

    assert!(php_pool.contains(&format!(
      "php_admin_value[mysqli.default_socket] = {}",
      system_socket.display()
    )));
    assert!(php_pool.contains(&format!(
      "php_admin_value[pdo_mysql.default_socket] = {}",
      system_socket.display()
    )));
    assert!(!php_pool.contains("services/mariadb/mariadb.sock"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn uses_managed_mariadb_socket_automatically_when_service_is_running() {
    let fixture_id = Uuid::new_v4().simple().to_string();
    let root = std::env::temp_dir().join(format!("fdmm-{}", &fixture_id[..8]));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let mariadb_server = mariadb_server_binary(&runtimes.mariadb);
    std::fs::create_dir_all(mariadb_server.parent().expect("MariaDB binary parent"))
      .expect("create MariaDB Runtime fixture");
    std::fs::write(mariadb_server, "").expect("create MariaDB server fixture");
    let managed_socket = paths.services.join("mariadb/mariadb.sock");
    std::fs::create_dir_all(managed_socket.parent().expect("Managed Socket parent"))
      .expect("create Managed Socket directory");
    let _managed_listener = std::os::unix::net::UnixListener::bind(&managed_socket)
      .expect("bind running Managed MariaDB Socket");
    let mut supervisor =
      ServiceSupervisor::new(paths.clone(), runtimes.clone(), ServicePorts::system());
    supervisor
      .save_mariadb_settings(MariaDbSettings {
        port: 3306,
        data_dir: root.join("unused-managed-data"),
        connection_mode: MariaDbConnectionMode::System,
        system_socket: root.join("system/mysql.sock"),
      })
      .expect("save legacy System MariaDB preference");

    generate_php_config(
      &paths,
      &runtimes,
      &"8.2".parse().expect("parse PHP version"),
    )
    .expect("generate PHP config with Managed MariaDB Runtime");
    let php_pool = std::fs::read_to_string(paths.services.join("php/8.2/php-fpm.d/www.conf"))
      .expect("read PHP-FPM pool");
    assert!(php_pool.contains(&format!(
      "php_admin_value[mysqli.default_socket] = {}",
      managed_socket.display()
    )));
    assert!(php_pool.contains(&format!(
      "php_admin_value[pdo_mysql.default_socket] = {}",
      managed_socket.display()
    )));
    assert!(!php_pool.contains("system/mysql.sock"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn refreshes_inactive_installed_php_configs_when_mariadb_connection_changes() {
    let fixture_id = Uuid::new_v4().simple().to_string();
    let root = std::env::temp_dir().join(format!("fdmi-{}", &fixture_id[..8]));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    add_php_runtime(&runtimes.php, "8.4.24");
    for version in ["8.2", "8.4"] {
      generate_php_config(
        &paths,
        &runtimes,
        &version.parse().expect("parse PHP version"),
      )
      .expect("generate stopped Managed MariaDB PHP config");
    }

    let managed_socket = paths.services.join("mariadb/mariadb.sock");
    std::fs::create_dir_all(managed_socket.parent().expect("Managed Socket parent"))
      .expect("create Managed Socket directory");
    let _managed_listener = std::os::unix::net::UnixListener::bind(&managed_socket)
      .expect("bind running Managed MariaDB Socket");
    let mut supervisor =
      ServiceSupervisor::new(paths.clone(), runtimes.clone(), ServicePorts::system());

    supervisor
      .refresh_php_mariadb_connection()
      .await
      .expect("refresh every installed PHP config");

    for version in ["8.2", "8.4"] {
      let php_pool = std::fs::read_to_string(
        paths
          .services
          .join(format!("php/{version}/php-fpm.d/www.conf")),
      )
      .expect("read refreshed inactive PHP-FPM pool");
      assert!(php_pool.contains(&format!(
        "php_admin_value[mysqli.default_socket] = {}",
        managed_socket.display()
      )));
      assert!(php_pool.contains(&format!(
        "php_admin_value[pdo_mysql.default_socket] = {}",
        managed_socket.display()
      )));
    }
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn uses_system_mariadb_socket_when_managed_service_is_not_running() {
    let root = std::env::temp_dir().join(format!("fabdev-no-managed-mariadb-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let mariadb_server = mariadb_server_binary(&runtimes.mariadb);
    std::fs::create_dir_all(mariadb_server.parent().expect("MariaDB binary parent"))
      .expect("create installed MariaDB Runtime fixture");
    std::fs::write(mariadb_server, "").expect("create installed MariaDB server fixture");

    generate_php_config(
      &paths,
      &runtimes,
      &"8.2".parse().expect("parse PHP version"),
    )
    .expect("generate PHP config while Managed MariaDB is stopped");
    let php_pool = std::fs::read_to_string(paths.services.join("php/8.2/php-fpm.d/www.conf"))
      .expect("read PHP-FPM pool");
    let system_socket = default_mariadb_system_socket();

    assert!(php_pool.contains(&format!(
      "php_admin_value[mysqli.default_socket] = {}",
      system_socket.display()
    )));
    assert!(php_pool.contains(&format!(
      "php_admin_value[pdo_mysql.default_socket] = {}",
      system_socket.display()
    )));
    assert!(!php_pool.contains("services/mariadb/mariadb.sock"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(unix)]
  #[test]
  fn detects_the_first_running_system_mariadb_socket() {
    let fixture_id = Uuid::new_v4().simple().to_string();
    let root = std::env::temp_dir().join(format!("fdms-{}", &fixture_id[..8]));
    std::fs::create_dir_all(&root).expect("create System MariaDB Socket fixture");
    let socket = root.join("mysql.sock");
    let _listener =
      std::os::unix::net::UnixListener::bind(&socket).expect("bind System MariaDB Socket fixture");
    let candidates = [root.join("missing.sock"), socket.clone()];

    assert_eq!(first_existing_unix_socket(&candidates), Some(socket));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn generates_static_site_without_php_runtime() {
    let root = std::env::temp_dir().join(format!("fabdev-static-site-{}", Uuid::new_v4()));
    let project = root.join("project/public");
    std::fs::create_dir_all(&project).expect("create fixture");
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    let site = Site {
      id: Uuid::new_v4(),
      name: "Static Demo".to_owned(),
      domain: "static-demo.test".to_owned(),
      project_path: root.join("project"),
      document_root: project,
      php_version: None,
      enabled: true,
      secured: false,
    };

    let generated = generate_configs(
      &paths,
      &runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
      &[site],
    )
    .expect("generate static config");
    let nginx_site = std::fs::read_to_string(paths.sites.join("static-demo.test.conf"))
      .expect("read static Site config");

    assert!(generated.php.is_empty());
    assert!(nginx_site.contains("index index.html;"));
    assert!(!nginx_site.contains("fastcgi_pass"));
    assert!(!paths.services.join("php").exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn applies_multiple_site_config_files_once() {
    let root = std::env::temp_dir().join(format!("fabdev-site-batch-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create batch fixture");
    let old_one = root.join("old-one.test.conf");
    let old_two = root.join("old-two.test.conf");
    let new_one = root.join("new-one.test.conf");
    let new_two = root.join("new-two.test.conf");
    std::fs::write(&old_one, "old one").expect("write first old config");
    std::fs::write(&old_two, "old two").expect("write second old config");
    let affected = [
      old_one.clone(),
      old_two.clone(),
      new_one.clone(),
      new_two.clone(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let rendered = vec![
      (new_one.clone(), "new one".to_owned()),
      (new_two.clone(), "new two".to_owned()),
    ];
    let apply_count = Cell::new(0);

    apply_site_config_files(&affected, &rendered, true, || {
      apply_count.set(apply_count.get() + 1);
      Ok(())
    })
    .expect("apply Site config batch");

    assert_eq!(apply_count.get(), 1);
    assert!(!old_one.exists());
    assert!(!old_two.exists());
    assert_eq!(
      std::fs::read_to_string(new_one).expect("read first new config"),
      "new one"
    );
    assert_eq!(
      std::fs::read_to_string(new_two).expect("read second new config"),
      "new two"
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn restores_site_config_batch_after_apply_failure() {
    let root = std::env::temp_dir().join(format!("fabdev-site-batch-rollback-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create rollback fixture");
    let existing = root.join("existing.test.conf");
    let added = root.join("added.test.conf");
    std::fs::write(&existing, "existing config").expect("write existing config");
    let affected = [existing.clone(), added.clone()]
      .into_iter()
      .collect::<BTreeSet<_>>();
    let rendered = vec![(added.clone(), "added config".to_owned())];
    let apply_count = Cell::new(0);

    let error = apply_site_config_files(&affected, &rendered, true, || {
      let count = apply_count.get() + 1;
      apply_count.set(count);
      if count == 1 {
        bail!("simulated Nginx reload failure");
      }
      Ok(())
    })
    .expect_err("reject failed Site config batch");

    assert!(error.to_string().contains("simulated Nginx reload failure"));
    assert_eq!(apply_count.get(), 2);
    assert_eq!(
      std::fs::read_to_string(existing).expect("read restored config"),
      "existing config"
    );
    assert!(!added.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn replaces_multiple_site_configs_before_services_start() {
    let root = std::env::temp_dir().join(format!("fabdev-site-batch-files-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create data directories");
    let old_project = root.join("old-project");
    let new_project = root.join("new-project");
    std::fs::create_dir_all(&old_project).expect("create old project");
    std::fs::create_dir_all(&new_project).expect("create new project");
    let old_site = Site {
      id: Uuid::new_v4(),
      name: "Old Site".to_owned(),
      domain: "old-site.test".to_owned(),
      project_path: old_project.clone(),
      document_root: old_project,
      php_version: None,
      enabled: true,
      secured: false,
    };
    let new_site = Site {
      id: old_site.id,
      name: "New Site".to_owned(),
      domain: "new-site.test".to_owned(),
      project_path: new_project.clone(),
      document_root: new_project,
      php_version: None,
      enabled: true,
      secured: false,
    };
    std::fs::write(paths.sites.join("old-site.test.conf"), "old config")
      .expect("write old Site config");
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      RuntimePaths {
        dnsmasq: root.join("runtimes/dnsmasq"),
        nginx: root.join("runtimes/nginx"),
        php: root.join("runtimes/php"),
        mariadb: root.join("runtimes/mariadb"),
      },
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    supervisor
      .apply_site_config_batch(
        std::slice::from_ref(&old_site),
        std::slice::from_ref(&new_site),
        std::slice::from_ref(&old_site),
        std::slice::from_ref(&new_site),
      )
      .await
      .expect("replace Site config batch");

    assert!(!paths.sites.join("old-site.test.conf").exists());
    let config = std::fs::read_to_string(paths.sites.join("new-site.test.conf"))
      .expect("read new Site config");
    assert!(config.contains("server_name new-site.test;"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn writes_site_config_before_services_start() {
    let root = std::env::temp_dir().join(format!("fabdev-add-site-{}", Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    let paths = AppPaths::from_root(root.join("data"));
    let site = Site {
      id: Uuid::new_v4(),
      name: "Site One".to_owned(),
      domain: "site-one.test".to_owned(),
      project_path: project.clone(),
      document_root: project,
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    };
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    supervisor
      .add_site_config(&site)
      .await
      .expect("write Site config");

    let config =
      std::fs::read_to_string(paths.sites.join("site-one.test.conf")).expect("read Site config");
    assert!(config.contains("server_name site-one.test;"));
    assert!(config.contains("listen 127.0.0.1:8080;"));
    let managed_php_ini = paths.config.join("php/8.2/php.ini");
    assert!(managed_php_ini.is_file());
    std::fs::write(&managed_php_ini, "[PHP]\nmemory_limit = 256M\n")
      .expect("write managed php.ini");
    supervisor
      .add_site_config(&site)
      .await
      .expect("regenerate Site config");
    assert_eq!(
      std::fs::read_to_string(paths.services.join("php/8.2/php.ini"))
        .expect("read service php.ini"),
      "[PHP]\nmemory_limit = 256M\n"
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn updates_site_domain_and_document_root_before_services_start() {
    let root = std::env::temp_dir().join(format!("fabdev-edit-site-{}", Uuid::new_v4()));
    let old_project = root.join("old-project");
    let new_project = root.join("new-project");
    std::fs::create_dir_all(old_project.join("public")).expect("create old project");
    std::fs::create_dir_all(new_project.join("web")).expect("create new project");
    let paths = AppPaths::from_root(root.join("data"));
    let previous = Site {
      id: Uuid::new_v4(),
      name: "Old ERP".to_owned(),
      domain: "old-erp.test".to_owned(),
      project_path: old_project.clone(),
      document_root: old_project.join("public"),
      php_version: None,
      enabled: true,
      secured: false,
    };
    let updated = Site {
      name: "New ERP".to_owned(),
      domain: "new-erp.test".to_owned(),
      project_path: new_project.clone(),
      document_root: new_project.join("web"),
      ..previous.clone()
    };
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      RuntimePaths {
        dnsmasq: root.join("runtimes/dnsmasq"),
        nginx: root.join("runtimes/nginx"),
        php: root.join("runtimes/php"),
        mariadb: root.join("runtimes/mariadb"),
      },
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor
      .add_site_config(&previous)
      .await
      .expect("write initial Site config");

    supervisor
      .update_site_config(&previous, &updated)
      .await
      .expect("update Site config");

    assert!(!paths.sites.join("old-erp.test.conf").exists());
    let config = std::fs::read_to_string(paths.sites.join("new-erp.test.conf"))
      .expect("read updated Site config");
    assert!(config.contains("server_name new-erp.test;"));
    assert!(config.contains(&new_project.join("web").to_string_lossy().replace('\\', "/")));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn switches_site_config_to_an_installed_php_version() {
    let root = std::env::temp_dir().join(format!("fabdev-switch-site-{}", Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    let paths = AppPaths::from_root(root.join("data"));
    let previous = Site {
      id: Uuid::new_v4(),
      name: "Site One".to_owned(),
      domain: "site-one.test".to_owned(),
      project_path: project.clone(),
      document_root: project,
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    };
    let mut updated = previous.clone();
    updated.php_version = Some(PhpVersion { major: 7, minor: 4 });
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    add_php_runtime(&runtimes.php, "7.4.33");
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor
      .add_site_config(&previous)
      .await
      .expect("write initial Site config");

    supervisor
      .update_site_php_config(&previous, &updated, std::slice::from_ref(&updated))
      .await
      .expect("switch Site PHP config");

    let config =
      std::fs::read_to_string(paths.sites.join("site-one.test.conf")).expect("read Site config");
    #[cfg(unix)]
    {
      assert!(config.contains("services/php/7.4/php-fpm.sock"));
      assert!(!config.contains("services/php/8.2/php-fpm.sock"));
    }
    #[cfg(windows)]
    {
      assert!(config.contains("fastcgi_pass 127.0.0.1:19074;"));
      assert!(!config.contains("fastcgi_pass 127.0.0.1:19082;"));
    }
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn switches_site_config_to_no_php() {
    let root = std::env::temp_dir().join(format!("fabdev-disable-site-php-{}", Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    let paths = AppPaths::from_root(root.join("data"));
    let previous = Site {
      id: Uuid::new_v4(),
      name: "Static".to_owned(),
      domain: "static.test".to_owned(),
      project_path: project.clone(),
      document_root: project,
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    };
    let mut updated = previous.clone();
    updated.php_version = None;
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      runtimes,
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor
      .add_site_config(&previous)
      .await
      .expect("write initial Site config");

    supervisor
      .update_site_php_config(&previous, &updated, std::slice::from_ref(&updated))
      .await
      .expect("disable Site PHP");

    let config =
      std::fs::read_to_string(paths.sites.join("static.test.conf")).expect("read Site config");
    assert!(config.contains("index index.html;"));
    assert!(!config.contains("fastcgi_pass"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn enables_and_disables_site_https_config() {
    let root = std::env::temp_dir().join(format!("fabdev-site-https-{}", Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("create project");
    let paths = AppPaths::from_root(root.join("data"));
    let site = Site {
      id: Uuid::new_v4(),
      name: "Secure ERP".to_owned(),
      domain: "secure-erp.test".to_owned(),
      project_path: project.clone(),
      document_root: project,
      php_version: None,
      enabled: true,
      secured: false,
    };
    let mut supervisor = ServiceSupervisor::new(
      paths.clone(),
      RuntimePaths {
        dnsmasq: root.join("runtimes/dnsmasq"),
        nginx: root.join("runtimes/nginx"),
        php: root.join("runtimes/php"),
        mariadb: root.join("runtimes/mariadb"),
      },
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );
    supervisor
      .add_site_config(&site)
      .await
      .expect("write initial config");

    let mut secured = site.clone();
    secured.secured = true;
    supervisor
      .update_site_https_config(&site, &secured)
      .await
      .expect("enable HTTPS");
    let config_path = paths.sites.join("secure-erp.test.conf");
    let secure_config = std::fs::read_to_string(&config_path).expect("read secure config");
    assert!(secure_config.contains("listen 127.0.0.1:8443 ssl;"));
    assert!(paths.config.join("tls/sites/secure-erp.test.crt").is_file());
    assert!(paths.config.join("tls/sites/secure-erp.test.key").is_file());

    supervisor
      .update_site_https_config(&secured, &site)
      .await
      .expect("disable HTTPS");
    let insecure_config = std::fs::read_to_string(config_path).expect("read insecure config");
    assert!(!insecure_config.contains("ssl_certificate"));
    assert!(!paths.config.join("tls/sites/secure-erp.test.crt").exists());
    assert!(!paths.config.join("tls/sites/secure-erp.test.key").exists());
    assert!(paths.config.join("tls/ca.crt").is_file());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn removes_only_the_site_config() {
    let root = std::env::temp_dir().join(format!("fabdev-remove-site-{}", Uuid::new_v4()));
    let project = root.join("project");
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create data directories");
    std::fs::create_dir_all(&project).expect("create project");
    std::fs::write(project.join("keep.txt"), "project data").expect("write project fixture");
    let site = Site {
      id: Uuid::new_v4(),
      name: "ERP Demo".to_owned(),
      domain: "erp-demo.test".to_owned(),
      project_path: project.clone(),
      document_root: project.clone(),
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    };
    let site_config = paths.sites.join("erp-demo.test.conf");
    std::fs::write(&site_config, "server {}").expect("write site config");
    let mut supervisor = ServiceSupervisor::new(
      paths,
      RuntimePaths {
        dnsmasq: root.join("runtimes/dnsmasq"),
        nginx: root.join("runtimes/nginx"),
        php: root.join("runtimes/php"),
        mariadb: root.join("runtimes/mariadb"),
      },
      ServicePorts {
        dns: 53535,
        http: 8080,
        https: 8443,
        mariadb: 3306,
      },
    );

    supervisor
      .remove_site_config(&site, &[])
      .await
      .expect("remove Site config");

    assert!(!site_config.exists());
    assert_eq!(
      std::fs::read_to_string(project.join("keep.txt")).expect("read project fixture"),
      "project data"
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn resolves_highest_installed_patch_for_minor_version() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-resolve-{}", Uuid::new_v4()));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("dnsmasq/current"),
      nginx: root.join("nginx/current"),
      php: root.join("php"),
      mariadb: root.join("mariadb/current"),
    };
    add_php_runtime(&runtimes.php, "8.2.31");
    let latest = add_php_runtime(&runtimes.php, "8.2.33");
    add_php_runtime(&runtimes.php, "8.3.20");

    assert_eq!(
      runtimes
        .resolve_php(&PhpVersion { major: 8, minor: 2 })
        .expect("resolve PHP 8.2"),
      latest
    );
    assert!(runtimes
      .resolve_php(&PhpVersion { major: 7, minor: 4 })
      .is_err());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn accepts_successful_dns_answer_for_expected_transaction() {
    let response = [
      0xFA, 0xBD, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    ];

    assert!(dns_response_is_success(&response, [0xFA, 0xBD]));
    assert!(!dns_response_is_success(&response, [0x00, 0x01]));
  }

  #[test]
  fn uses_erp_php_ini_defaults_for_php_84() {
    let version: PhpVersion = "8.4".parse().expect("parse PHP version");
    let contents = php_ini_template(&version);

    assert!(contents.contains("date.timezone = \"Asia/Taipei\""));
    assert!(contents.contains("upload_max_filesize = 64M"));
    assert!(contents.contains("post_max_size = 64M"));
  }

  #[test]
  fn renders_php_82_erp_defaults_for_an_installed_php_series() {
    let root = std::env::temp_dir().join(format!("fabdev-erp-php-ini-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "7.4.33");
    let supervisor = ServiceSupervisor::new(paths, runtimes, ServicePorts::system());
    let version: PhpVersion = "7.4".parse().expect("parse PHP 7.4");

    let contents = supervisor
      .read_erp_php_ini(Some(&version))
      .expect("render PHP 8.2 ERP defaults for PHP 7.4");

    assert!(contents.contains("date.timezone = \"Asia/Taipei\""));
    assert!(contents.contains("max_input_vars = 99999"));
    assert!(contents.replace('\\', "/").contains("runtimes/php/7.4.33"));
    assert!(!contents.contains("@RUNTIME_ROOT@"));
    std::fs::remove_dir_all(root).expect("remove ERP php.ini fixture");
  }

  #[test]
  fn preserves_an_empty_managed_php_ini() {
    let root = std::env::temp_dir().join(format!("fabdev-empty-php-ini-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    let version: PhpVersion = "8.2".parse().expect("parse PHP 8.2");
    let managed = managed_php_ini_path(&paths, &version);
    std::fs::create_dir_all(managed.parent().expect("managed php.ini parent"))
      .expect("create managed php.ini directory");
    std::fs::write(&managed, "").expect("write empty managed php.ini");

    let generated = generate_php_config(&paths, &runtimes, &version)
      .expect("generate service config with empty php.ini");
    let contents = std::fs::read_to_string(&managed).expect("read managed php.ini");
    let service_contents =
      std::fs::read_to_string(generated.php_ini).expect("read service php.ini");

    assert!(contents.is_empty());
    assert!(service_contents.is_empty());
    std::fs::remove_dir_all(root).expect("remove empty php.ini fixture");
  }

  #[test]
  fn renders_windows_service_defaults_for_an_empty_managed_php_ini() {
    let rendered = effective_php_ini_contents("", true, &|template| {
      template
        .replace("@RUNTIME_ROOT@", "C:/fabdev/php/8.4.24")
        .replace("@SERVICE_ROOT@", "C:/fabdev/services/php/8.4")
        .replace("@MARIADB_SOCKET@", "")
        .replace("@PHP_EXTENSION_API@", "")
    });

    assert!(rendered.contains("extension_dir = \"C:/fabdev/php/8.4.24/ext\""));
    assert!(rendered.contains("extension = mysqli"));
    assert!(rendered.contains("extension = pdo_mysql"));
  }

  #[test]
  fn preserves_nonempty_windows_managed_php_ini() {
    let managed = "memory_limit = 256M\n";
    let rendered = effective_php_ini_contents(managed, true, &|_| {
      panic!("the Windows template must not replace user contents")
    });

    assert_eq!(rendered, managed);
  }

  #[cfg(unix)]
  #[test]
  fn initializes_and_preserves_online_runtime_php_ini() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("fabdev-online-php-ini-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    paths.ensure().expect("create App paths");
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    let runtime = add_php_runtime(&runtimes.php, "8.4.24");
    let cli = runtime.join("bin/php");
    std::fs::create_dir_all(cli.parent().expect("CLI parent")).expect("create CLI parent");
    for executable in [&cli, &runtime.join("sbin/php-fpm")] {
      std::fs::write(executable, "#!/bin/sh\nexit 0\n").expect("write PHP fixture");
      let mut permissions = std::fs::metadata(executable)
        .expect("read PHP fixture metadata")
        .permissions();
      permissions.set_mode(0o755);
      std::fs::set_permissions(executable, permissions).expect("make PHP fixture executable");
    }
    let supervisor = ServiceSupervisor::new(paths.clone(), runtimes, ServicePorts::system());
    let version: PhpVersion = "8.4".parse().expect("parse PHP 8.4");
    let managed = managed_php_ini_path(&paths, &version);

    supervisor
      .validate_php_runtime_install(&version, "8.4.24")
      .expect("initialize online Runtime php.ini");
    assert_eq!(std::fs::read_to_string(&managed).expect("read php.ini"), "");
    std::fs::write(&managed, "memory_limit = 256M\n").expect("customize php.ini");
    supervisor
      .validate_php_runtime_install(&version, "8.4.24")
      .expect("revalidate online Runtime php.ini");
    assert_eq!(
      std::fs::read_to_string(&managed).expect("read preserved php.ini"),
      "memory_limit = 256M\n"
    );
    std::fs::remove_dir_all(root).expect("remove online php.ini fixture");
  }

  #[test]
  fn initializes_default_php_ini_from_current_php_82_and_reuses_it() {
    let root = std::env::temp_dir().join(format!("fabdev-default-php-ini-{}", Uuid::new_v4()));
    let paths = AppPaths::from_root(root.join("data"));
    let runtimes = RuntimePaths {
      dnsmasq: root.join("runtimes/dnsmasq"),
      nginx: root.join("runtimes/nginx"),
      php: root.join("runtimes/php"),
      mariadb: root.join("runtimes/mariadb"),
    };
    add_php_runtime(&runtimes.php, "8.2.33");
    add_php_runtime(&runtimes.php, "8.4.24");
    let php_82: PhpVersion = "8.2".parse().expect("parse PHP 8.2");
    let php_84: PhpVersion = "8.4".parse().expect("parse PHP 8.4");

    generate_php_config(&paths, &runtimes, &php_82).expect("generate PHP 8.2 config");
    let php_82_ini = managed_php_ini_path(&paths, &php_82);
    let customized = std::fs::read_to_string(&php_82_ini)
      .expect("read PHP 8.2 config")
      .replace("memory_limit = 128M", "memory_limit = 256M");
    std::fs::write(&php_82_ini, customized).expect("customize PHP 8.2 config");
    std::fs::remove_file(default_php_ini_path(&paths)).expect("remove initial default template");

    let generated = generate_php_config(&paths, &runtimes, &php_84)
      .expect("generate PHP 8.4 config from default");
    let template =
      std::fs::read_to_string(default_php_ini_path(&paths)).expect("read default PHP template");
    let php_84_ini =
      std::fs::read_to_string(generated.php_ini).expect("read generated PHP 8.4 config");

    assert!(template.contains("memory_limit = 256M"));
    assert!(template.contains("@RUNTIME_ROOT@"));
    assert!(template.contains("@SERVICE_ROOT@"));
    assert!(!template.contains(root.to_string_lossy().as_ref()));
    assert!(php_84_ini.contains("memory_limit = 256M"));
    let normalized_php_84_ini = php_84_ini.replace('\\', "/");
    assert!(normalized_php_84_ini.contains("runtimes/php/8.4.24"));
    assert!(normalized_php_84_ini.contains("services/php/8.4"));
    assert!(std::fs::read_to_string(php_82_ini)
      .expect("read preserved PHP 8.2 config")
      .contains("memory_limit = 256M"));
    std::fs::remove_dir_all(root).expect("remove default PHP fixture");
  }
}
