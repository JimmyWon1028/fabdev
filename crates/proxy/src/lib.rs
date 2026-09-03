use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use fabdev_core::{
  normalize_domain, ProxyConnectionInfo, ProxyConnectionInput, ProxyConnectionSettings,
  ProxyConnectionState, ProxyManagerState, DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS,
  MAX_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS,
};
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{
  ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
  ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_MAX_AGE, ACCESS_CONTROL_REQUEST_HEADERS,
  ACCESS_CONTROL_REQUEST_METHOD, HOST, ORIGIN, VARY,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::task::{JoinHandle, JoinSet};

const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const UPSTREAM_HEALTH_INTERVAL: Duration = Duration::from_secs(15);
const UPSTREAM_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const UPSTREAM_HEALTH_FAILURE_THRESHOLD: u8 = 3;
const UPSTREAM_HEALTH_RECOVERY_THRESHOLD: u8 = 2;
const UPSTREAM_REQUEST_RECOVERY_THRESHOLD: u8 = 2;

type ProxyBody = BoxBody<Bytes, hyper::Error>;
type ProxyClient = Client<HttpConnector, Incoming>;

struct ConnectionHealth {
  connection_id: String,
  target_authority: String,
  state: Mutex<ConnectionHealthState>,
}

#[derive(Default)]
struct ConnectionHealthState {
  periodic_error: Option<String>,
  periodic_failures: u8,
  periodic_successes: u8,
  request_error: Option<String>,
  request_successes: u8,
  runtime_error: Option<String>,
}

struct UpstreamHealthFailure {
  message: String,
  error_kind: &'static str,
  os_error_code: Option<i32>,
}

impl ConnectionHealth {
  fn new(settings: &ProxyConnectionSettings) -> Self {
    Self {
      connection_id: settings.id.clone(),
      target_authority: masked_target_authority(&settings.target),
      state: Mutex::new(ConnectionHealthState::default()),
    }
  }

  async fn record_periodic_failure(&self, failure: UpstreamHealthFailure, elapsed: Duration) {
    let mut state = self.state.lock().await;
    state.periodic_successes = 0;
    state.periodic_failures = state.periodic_failures.saturating_add(1);
    if state.periodic_failures < UPSTREAM_HEALTH_FAILURE_THRESHOLD {
      return;
    }
    let entering_degraded = state.periodic_error.is_none();
    state.periodic_error = Some(failure.message.clone());
    if entering_degraded {
      self.log_transition(
        "tcp",
        "degraded",
        elapsed,
        failure.error_kind,
        failure.os_error_code,
        Some(&failure.message),
      );
    }
  }

  async fn record_periodic_success(&self, elapsed: Duration) {
    let mut state = self.state.lock().await;
    state.periodic_failures = 0;
    if state.periodic_error.is_none() {
      state.periodic_successes = 0;
      return;
    }
    state.periodic_successes = state.periodic_successes.saturating_add(1);
    if state.periodic_successes < UPSTREAM_HEALTH_RECOVERY_THRESHOLD {
      return;
    }
    state.periodic_error = None;
    state.periodic_successes = 0;
    self.log_transition("tcp", "recovered", elapsed, "none", None, None);
  }

  async fn record_request_failure(
    &self,
    message: String,
    error_kind: &'static str,
    elapsed: Duration,
  ) {
    let mut state = self.state.lock().await;
    state.request_successes = 0;
    let entering_degraded = state.request_error.is_none();
    state.request_error = Some(message.clone());
    if entering_degraded {
      self.log_transition(
        "request",
        "degraded",
        elapsed,
        error_kind,
        None,
        Some(&message),
      );
    }
  }

  async fn record_request_success(&self, elapsed: Duration) {
    let mut state = self.state.lock().await;
    if state.request_error.is_none() {
      state.request_successes = 0;
      return;
    }
    state.request_successes = state.request_successes.saturating_add(1);
    if state.request_successes < UPSTREAM_REQUEST_RECOVERY_THRESHOLD {
      return;
    }
    state.request_error = None;
    state.request_successes = 0;
    self.log_transition("request", "recovered", elapsed, "none", None, None);
  }

  async fn record_runtime_failure(&self, message: String) {
    let mut state = self.state.lock().await;
    let entering_degraded = state.runtime_error.is_none();
    state.runtime_error = Some(message.clone());
    if entering_degraded {
      self.log_transition(
        "listener",
        "degraded",
        Duration::ZERO,
        "accept",
        None,
        Some(&message),
      );
    }
  }

  async fn record_runtime_success(&self) {
    let mut state = self.state.lock().await;
    if state.runtime_error.take().is_some() {
      self.log_transition("listener", "recovered", Duration::ZERO, "none", None, None);
    }
  }

  async fn last_error(&self) -> Option<String> {
    let state = self.state.lock().await;
    state
      .runtime_error
      .clone()
      .or_else(|| state.request_error.clone())
      .or_else(|| state.periodic_error.clone())
  }

  fn log_transition(
    &self,
    check_type: &str,
    state: &str,
    elapsed: Duration,
    error_kind: &str,
    os_error_code: Option<i32>,
    message: Option<&str>,
  ) {
    let timestamp_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap_or_default()
      .as_millis();
    eprintln!(
      "Proxy health transition timestamp_ms={timestamp_ms} connection_id={} target_authority={} check_type={check_type} elapsed_ms={} error_kind={error_kind} os_error_code={} state={state} message={}",
      self.connection_id,
      self.target_authority,
      elapsed.as_millis(),
      os_error_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_owned()),
      message.unwrap_or("none")
    );
  }
}

struct RunningProxy {
  stop: Option<oneshot::Sender<()>>,
  task: JoinHandle<()>,
  health: Arc<ConnectionHealth>,
}

pub struct ProxyManager {
  connections: BTreeMap<String, ProxyConnectionSettings>,
  running: HashMap<String, RunningProxy>,
  failed: HashMap<String, String>,
}

impl ProxyManager {
  pub fn new(connections: Vec<ProxyConnectionSettings>) -> Result<Self> {
    let mut manager = Self {
      connections: BTreeMap::new(),
      running: HashMap::new(),
      failed: HashMap::new(),
    };
    for connection in connections {
      manager.add_settings(connection)?;
    }
    Ok(manager)
  }

  pub fn connections(&self) -> Vec<ProxyConnectionSettings> {
    let mut connections = self.connections.values().cloned().collect::<Vec<_>>();
    connections.sort_by_key(|connection| connection.listen_port);
    connections
  }

  pub fn add(&mut self, input: ProxyConnectionInput) -> Result<String> {
    let settings = connection_from_input(input)?;
    let id = settings.id.clone();
    self.add_settings(settings)?;
    Ok(id)
  }

  pub async fn update(
    &mut self,
    id: &str,
    input: ProxyConnectionInput,
  ) -> Result<(ProxyConnectionSettings, bool)> {
    let previous = self
      .connections
      .get(id)
      .with_context(|| format!("unknown Proxy connection: {id}"))?
      .clone();
    let settings = connection_from_input(input)?;
    if settings.id != id {
      bail!("Proxy connection ID cannot be changed");
    }
    self.validate_unique(&settings, Some(id))?;
    let was_running = self.running.contains_key(id);
    if was_running {
      self.stop(id).await?;
    }
    self.failed.remove(id);
    self.connections.insert(id.to_owned(), settings);
    if was_running {
      self.start(id).await?;
    }
    Ok((previous, was_running))
  }

  pub async fn restore_update(
    &mut self,
    mut settings: ProxyConnectionSettings,
    should_run: bool,
  ) -> Result<()> {
    normalize_connection_settings(&mut settings);
    let id = settings.id.clone();
    validate_connection(&settings)?;
    self.validate_unique(&settings, Some(&id))?;
    self.stop(&id).await?;
    self.failed.remove(&id);
    self.connections.insert(id.clone(), settings);
    if should_run {
      self.start(&id).await?;
    }
    Ok(())
  }

  pub async fn remove(&mut self, id: &str) -> Result<ProxyConnectionSettings> {
    let settings = self
      .connections
      .get(id)
      .with_context(|| format!("unknown Proxy connection: {id}"))?
      .clone();
    self.stop(id).await?;
    self.failed.remove(id);
    self.connections.remove(id);
    Ok(settings)
  }

  pub fn restore(&mut self, settings: ProxyConnectionSettings) -> Result<()> {
    self.add_settings(settings)
  }

  fn add_settings(&mut self, mut settings: ProxyConnectionSettings) -> Result<()> {
    normalize_connection_settings(&mut settings);
    validate_connection(&settings)?;
    self.validate_unique(&settings, None)?;
    self.connections.insert(settings.id.clone(), settings);
    Ok(())
  }

  fn validate_unique(
    &self,
    settings: &ProxyConnectionSettings,
    replacing_id: Option<&str>,
  ) -> Result<()> {
    if self.connections.contains_key(&settings.id) && replacing_id != Some(settings.id.as_str()) {
      bail!("duplicate Proxy connection id: {}", settings.id);
    }
    if let Some(existing) = self.connections.values().find(|connection| {
      replacing_id != Some(connection.id.as_str()) && connection.listen_port == settings.listen_port
    }) {
      bail!(
        "Proxy connections {} and {} both use port {}",
        existing.id,
        settings.id,
        settings.listen_port
      );
    }
    if let Some(existing) = self.connections.values().find(|connection| {
      replacing_id != Some(connection.id.as_str()) && connection.domain == settings.domain
    }) {
      bail!(
        "Proxy connections {} and {} both use domain {}",
        existing.id,
        settings.id,
        settings.domain
      );
    }
    Ok(())
  }

  pub async fn state(&self) -> ProxyManagerState {
    let mut connections = Vec::with_capacity(self.connections.len());
    for (id, settings) in &self.connections {
      let (state, last_error) = if let Some(running) = self.running.get(id) {
        if running.task.is_finished() {
          (
            ProxyConnectionState::Failed,
            Some("Proxy Runtime stopped unexpectedly".to_owned()),
          )
        } else {
          match running.health.last_error().await {
            Some(error) => (ProxyConnectionState::Degraded, Some(error)),
            None => (ProxyConnectionState::Running, None),
          }
        }
      } else if let Some(error) = self.failed.get(id) {
        (ProxyConnectionState::Failed, Some(error.clone()))
      } else {
        (ProxyConnectionState::Stopped, None)
      };
      connections.push(ProxyConnectionInfo {
        id: settings.id.clone(),
        name: settings.name.clone(),
        domain: settings.domain.clone(),
        listen_host: settings.listen_host.clone(),
        listen_port: settings.listen_port,
        target: settings.target.clone(),
        allowed_origins: settings.allowed_origins.clone(),
        upstream_response_timeout_seconds: settings.upstream_response_timeout_seconds,
        state,
        last_error,
      });
    }
    connections.sort_by_key(|connection| connection.listen_port);
    ProxyManagerState { connections }
  }

  pub fn running_ids(&self) -> Vec<String> {
    let mut ids = self.running.keys().cloned().collect::<Vec<_>>();
    ids.sort();
    ids
  }

  pub async fn start(&mut self, id: &str) -> Result<()> {
    if self.running.contains_key(id) {
      return Ok(());
    }
    let settings = self
      .connections
      .get(id)
      .with_context(|| format!("unknown Proxy connection: {id}"))?
      .clone();
    self.failed.remove(id);

    let address = proxy_listen_address(&settings)?;
    let listener = match TcpListener::bind(address).await {
      Ok(listener) => listener,
      Err(error) => {
        self.failed.insert(
          id.to_owned(),
          format!("unable to listen at {address}: {error}"),
        );
        return Ok(());
      }
    };
    let health = Arc::new(ConnectionHealth::new(&settings));
    let (stop, stop_receiver) = oneshot::channel();
    let task_health = Arc::clone(&health);
    let task = tokio::spawn(async move {
      run_proxy(listener, settings, task_health, stop_receiver).await;
    });
    self.running.insert(
      id.to_owned(),
      RunningProxy {
        stop: Some(stop),
        task,
        health,
      },
    );
    Ok(())
  }

  pub async fn stop(&mut self, id: &str) -> Result<()> {
    if !self.connections.contains_key(id) {
      bail!("unknown Proxy connection: {id}");
    }
    self.failed.remove(id);
    let Some(mut running) = self.running.remove(id) else {
      return Ok(());
    };
    if let Some(stop) = running.stop.take() {
      let _ = stop.send(());
    }
    if tokio::time::timeout(
      CONNECTION_DRAIN_TIMEOUT + Duration::from_secs(1),
      &mut running.task,
    )
    .await
    .is_err()
    {
      running.task.abort();
      let _ = running.task.await;
    }
    Ok(())
  }

  pub async fn start_all(&mut self) -> ProxyManagerState {
    let ids = self.connections.keys().cloned().collect::<Vec<_>>();
    for id in ids {
      if let Err(error) = self.start(&id).await {
        self.failed.insert(id, error.to_string());
      }
    }
    self.state().await
  }

  pub async fn stop_all(&mut self) -> ProxyManagerState {
    let ids = self.connections.keys().cloned().collect::<Vec<_>>();
    for id in ids {
      if let Err(error) = self.stop(&id).await {
        self.failed.insert(id, error.to_string());
      }
    }
    self.state().await
  }
}

impl Drop for ProxyManager {
  fn drop(&mut self) {
    for (_, mut running) in self.running.drain() {
      if let Some(stop) = running.stop.take() {
        let _ = stop.send(());
      }
      running.task.abort();
    }
  }
}

fn connection(
  id: &str,
  name: &str,
  domain: &str,
  listen_port: u16,
  target: &str,
) -> ProxyConnectionSettings {
  ProxyConnectionSettings {
    id: id.to_owned(),
    name: name.to_owned(),
    domain: domain.to_owned(),
    listen_host: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST).to_string(),
    listen_port,
    target: target.to_owned(),
    allowed_origins: vec![format!("http://{domain}"), format!("https://{domain}")],
    upstream_response_timeout_seconds: DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS,
  }
}

fn connection_from_input(input: ProxyConnectionInput) -> Result<ProxyConnectionSettings> {
  let id = input.id.trim().to_ascii_lowercase();
  let domain = normalize_domain(&input.domain)
    .with_context(|| format!("invalid Proxy domain: {}", input.domain))?;
  let target = input.target.trim().trim_end_matches('/').to_owned();
  let mut settings = connection(
    &id,
    &id.to_ascii_uppercase(),
    &domain,
    input.listen_port,
    &target,
  );
  settings.allowed_origins = normalize_allowed_origins(input.allowed_origins, &domain)?;
  settings.upstream_response_timeout_seconds = input
    .upstream_response_timeout_seconds
    .filter(|seconds| *seconds != 0)
    .unwrap_or(DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS);
  validate_connection(&settings)?;
  Ok(settings)
}

fn normalize_connection_settings(settings: &mut ProxyConnectionSettings) {
  if settings.upstream_response_timeout_seconds == 0 {
    settings.upstream_response_timeout_seconds = DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS;
  }
}

fn normalize_allowed_origins(origins: Vec<String>, domain: &str) -> Result<Vec<String>> {
  let origins = if origins.is_empty() {
    vec![format!("http://{domain}"), format!("https://{domain}")]
  } else {
    origins
  };
  let mut normalized = origins
    .into_iter()
    .map(|origin| {
      let origin = origin.trim();
      let uri: Uri = origin
        .parse()
        .with_context(|| format!("invalid Proxy allowed origin: {origin}"))?;
      let scheme = uri
        .scheme_str()
        .filter(|scheme| matches!(*scheme, "http" | "https"))
        .with_context(|| format!("Proxy allowed origin must use http or https: {origin}"))?;
      let authority = uri
        .authority()
        .with_context(|| format!("Proxy allowed origin has no host: {origin}"))?;
      if uri
        .path_and_query()
        .is_some_and(|path| path.as_str() != "/")
      {
        bail!("Proxy allowed origin cannot include a path or query: {origin}");
      }
      Ok(format!("{scheme}://{authority}"))
    })
    .collect::<Result<Vec<_>>>()?;
  normalized.sort();
  normalized.dedup();
  Ok(normalized)
}

fn validate_connection(settings: &ProxyConnectionSettings) -> Result<()> {
  if settings.id.is_empty()
    || !settings.id.chars().all(|character| {
      character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
  {
    bail!("invalid Proxy connection id: {}", settings.id);
  }
  if settings.listen_port < 1024 {
    bail!("Proxy listen port must be between 1024 and 65535");
  }
  if settings.upstream_response_timeout_seconds > MAX_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS {
    bail!(
      "Proxy upstream response timeout must be between 1 and {} seconds after defaults are applied",
      MAX_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS
    );
  }
  normalize_domain(&settings.domain)
    .with_context(|| format!("invalid Proxy domain: {}", settings.domain))?;
  let address = proxy_listen_address(settings)?;
  if !address.ip().is_loopback() {
    bail!("Proxy listeners must bind to loopback: {address}");
  }
  let target: Uri = settings
    .target
    .parse()
    .with_context(|| format!("invalid Proxy target: {}", settings.target))?;
  if target.scheme_str() != Some("http") || target.authority().is_none() {
    bail!(
      "Proxy target must be an absolute HTTP URL: {}",
      settings.target
    );
  }
  Ok(())
}

fn proxy_listen_address(settings: &ProxyConnectionSettings) -> Result<SocketAddr> {
  let host: IpAddr = settings
    .listen_host
    .parse()
    .with_context(|| format!("invalid Proxy listen host: {}", settings.listen_host))?;
  Ok(SocketAddr::new(host, settings.listen_port))
}

fn health_initial_delay(connection_id: &str) -> Duration {
  let interval_millis = UPSTREAM_HEALTH_INTERVAL.as_millis() as u64;
  let offset = connection_id.bytes().fold(0_u64, |hash, byte| {
    hash.wrapping_mul(31).wrapping_add(u64::from(byte))
  });
  Duration::from_millis(1 + offset % interval_millis.saturating_sub(1).max(1))
}

fn masked_target_authority(target: &str) -> String {
  let Ok(uri) = target.parse::<Uri>() else {
    return "invalid".to_owned();
  };
  let Some(authority) = uri.authority() else {
    return "invalid".to_owned();
  };
  let host = authority.host();
  let host = if host.contains(':') {
    format!("[{host}]")
  } else {
    host.to_owned()
  };
  match authority.port_u16() {
    Some(port) => format!("{host}:{port}"),
    None => host,
  }
}

fn classify_io_error(error: &std::io::Error) -> &'static str {
  if matches!(error.raw_os_error(), Some(11_001..=11_004)) {
    return "dns";
  }
  match error.kind() {
    std::io::ErrorKind::NotFound => "dns",
    std::io::ErrorKind::ConnectionRefused => "connection_refused",
    std::io::ErrorKind::ConnectionReset => "connection_reset",
    std::io::ErrorKind::ConnectionAborted => "connection_aborted",
    std::io::ErrorKind::NotConnected => "not_connected",
    std::io::ErrorKind::TimedOut => "timeout",
    std::io::ErrorKind::AddrNotAvailable => "address_unavailable",
    std::io::ErrorKind::NetworkUnreachable => "network_unreachable",
    std::io::ErrorKind::HostUnreachable => "host_unreachable",
    _ => "io",
  }
}

async fn run_proxy(
  listener: TcpListener,
  settings: ProxyConnectionSettings,
  health: Arc<ConnectionHealth>,
  mut stop: oneshot::Receiver<()>,
) {
  let mut connector = HttpConnector::new();
  connector.enforce_http(true);
  connector.set_connect_timeout(Some(UPSTREAM_CONNECT_TIMEOUT));
  let client: ProxyClient = Client::builder(TokioExecutor::new()).build(connector);
  let mut connections = JoinSet::new();
  let mut health_interval = tokio::time::interval_at(
    tokio::time::Instant::now() + health_initial_delay(&settings.id),
    UPSTREAM_HEALTH_INTERVAL,
  );
  health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

  loop {
    tokio::select! {
      _ = &mut stop => break,
      _ = health_interval.tick() => {
        let started = Instant::now();
        match check_upstream(&settings).await {
          Ok(()) => health.record_periodic_success(started.elapsed()).await,
          Err(error) => health.record_periodic_failure(error, started.elapsed()).await,
        }
      }
      accepted = listener.accept() => match accepted {
        Ok((stream, _)) => {
          health.record_runtime_success().await;
          let client = client.clone();
          let settings = settings.clone();
          let health = Arc::clone(&health);
          connections.spawn(async move {
            serve_client(stream, client, settings, health).await;
          });
        }
        Err(error) => {
          health
            .record_runtime_failure(format!("unable to accept Proxy connection: {error}"))
            .await;
          tokio::time::sleep(Duration::from_millis(100)).await;
        }
      }
    }
  }

  let drain = async { while connections.join_next().await.is_some() {} };
  if tokio::time::timeout(CONNECTION_DRAIN_TIMEOUT, drain)
    .await
    .is_err()
  {
    connections.abort_all();
    while connections.join_next().await.is_some() {}
  }
}

async fn check_upstream(
  settings: &ProxyConnectionSettings,
) -> std::result::Result<(), UpstreamHealthFailure> {
  let target: Uri = settings.target.parse().map_err(|_| UpstreamHealthFailure {
    message: "Proxy upstream health check has an invalid target".to_owned(),
    error_kind: "invalid_target",
    os_error_code: None,
  })?;
  let authority = target.authority().ok_or_else(|| UpstreamHealthFailure {
    message: "Proxy upstream health check target has no authority".to_owned(),
    error_kind: "invalid_target",
    os_error_code: None,
  })?;
  let host = authority.host();
  let port = authority.port_u16().unwrap_or(80);
  match tokio::time::timeout(UPSTREAM_HEALTH_TIMEOUT, TcpStream::connect((host, port))).await {
    Ok(Ok(_)) => Ok(()),
    Ok(Err(error)) => Err(UpstreamHealthFailure {
      message: format!("Proxy upstream health check failed: {error}"),
      error_kind: classify_io_error(&error),
      os_error_code: error.raw_os_error(),
    }),
    Err(_) => Err(UpstreamHealthFailure {
      message: format!(
        "Proxy upstream health check timed out after {} seconds",
        UPSTREAM_HEALTH_TIMEOUT.as_secs()
      ),
      error_kind: "timeout",
      os_error_code: None,
    }),
  }
}

async fn serve_client(
  stream: TcpStream,
  client: ProxyClient,
  settings: ProxyConnectionSettings,
  health: Arc<ConnectionHealth>,
) {
  let service = service_fn(move |request| {
    proxy_request(
      request,
      client.clone(),
      settings.clone(),
      Arc::clone(&health),
    )
  });
  let _ = http1::Builder::new()
    .serve_connection(TokioIo::new(stream), service)
    .with_upgrades()
    .await;
}

async fn proxy_request(
  mut request: Request<Incoming>,
  client: ProxyClient,
  settings: ProxyConnectionSettings,
  health: Arc<ConnectionHealth>,
) -> Result<Response<ProxyBody>, Infallible> {
  let allowed_origin = request
    .headers()
    .get(ORIGIN)
    .and_then(|origin| origin.to_str().ok())
    .filter(|origin| {
      settings
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
    })
    .map(str::to_owned);

  if request.method() == Method::OPTIONS && allowed_origin.is_some() {
    let requested_method = request
      .headers()
      .get(ACCESS_CONTROL_REQUEST_METHOD)
      .cloned();
    let requested_headers = request
      .headers()
      .get(ACCESS_CONTROL_REQUEST_HEADERS)
      .cloned();
    let mut response = Response::new(empty_body());
    *response.status_mut() = StatusCode::NO_CONTENT;
    apply_cors_headers(
      &mut response,
      allowed_origin.as_deref(),
      requested_method,
      requested_headers,
      true,
    );
    return Ok(response);
  }

  let path = request
    .uri()
    .path_and_query()
    .map(|path| path.as_str())
    .unwrap_or("/");
  let target_uri = format!("{}{path}", settings.target.trim_end_matches('/'));
  let target_uri: Uri = match target_uri.parse() {
    Ok(uri) => uri,
    Err(error) => {
      let message = format!("unable to build Proxy target URI: {error}");
      health
        .record_request_failure(message.clone(), "invalid_target", Duration::ZERO)
        .await;
      return Ok(error_response(StatusCode::BAD_GATEWAY, &message));
    }
  };
  let Some(authority) = target_uri.authority().cloned() else {
    let message = "Proxy target URI has no authority".to_owned();
    health
      .record_request_failure(message.clone(), "invalid_target", Duration::ZERO)
      .await;
    return Ok(error_response(StatusCode::BAD_GATEWAY, &message));
  };
  *request.uri_mut() = target_uri;
  if let Ok(host) = authority.as_str().parse() {
    request.headers_mut().insert(HOST, host);
  }

  let response_timeout = Duration::from_secs(settings.upstream_response_timeout_seconds.into());
  let request_started = Instant::now();
  let upstream = tokio::time::timeout(response_timeout, client.request(request)).await;
  let mut response = match upstream {
    Ok(Ok(response)) => {
      health
        .record_request_success(request_started.elapsed())
        .await;
      response.map(|body| body.boxed())
    }
    Ok(Err(error)) => {
      let message = format!("Proxy upstream request failed: {error}");
      health
        .record_request_failure(message.clone(), "request", request_started.elapsed())
        .await;
      error_response(StatusCode::BAD_GATEWAY, &message)
    }
    Err(_) => {
      let message = format!(
        "Proxy upstream response timed out after {} seconds",
        settings.upstream_response_timeout_seconds
      );
      health
        .record_request_failure(
          message.clone(),
          "response_timeout",
          request_started.elapsed(),
        )
        .await;
      error_response(StatusCode::GATEWAY_TIMEOUT, &message)
    }
  };
  apply_cors_headers(&mut response, allowed_origin.as_deref(), None, None, false);
  Ok(response)
}

fn apply_cors_headers(
  response: &mut Response<ProxyBody>,
  origin: Option<&str>,
  requested_method: Option<hyper::header::HeaderValue>,
  requested_headers: Option<hyper::header::HeaderValue>,
  preflight: bool,
) {
  let Some(origin) = origin.and_then(|origin| origin.parse().ok()) else {
    return;
  };
  let headers = response.headers_mut();
  headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
  headers.insert(ACCESS_CONTROL_ALLOW_CREDENTIALS, "true".parse().unwrap());
  headers.append(VARY, "Origin".parse().unwrap());
  if !preflight {
    return;
  }
  if let Some(method) = requested_method {
    headers.insert(ACCESS_CONTROL_ALLOW_METHODS, method);
  } else {
    headers.insert(
      ACCESS_CONTROL_ALLOW_METHODS,
      "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"
        .parse()
        .unwrap(),
    );
  }
  if let Some(requested_headers) = requested_headers {
    headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, requested_headers);
  } else {
    headers.insert(
      ACCESS_CONTROL_ALLOW_HEADERS,
      "Accept, Authorization, Content-Type".parse().unwrap(),
    );
  }
  headers.insert(ACCESS_CONTROL_MAX_AGE, "600".parse().unwrap());
}

fn empty_body() -> ProxyBody {
  Full::new(Bytes::new())
    .map_err(|error: Infallible| match error {})
    .boxed()
}

fn error_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
  let mut response = Response::new(
    Full::new(Bytes::copy_from_slice(message.as_bytes()))
      .map_err(|error: Infallible| match error {})
      .boxed(),
  );
  *response.status_mut() = status;
  response
}

#[cfg(test)]
mod tests {
  use std::net::{Ipv4Addr, TcpListener as StdTcpListener};

  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  use super::*;

  fn reserve_port() -> u16 {
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
      .expect("reserve local port")
      .local_addr()
      .expect("read reserved port")
      .port()
  }

  fn test_connection(id: &str, port: u16, target: String) -> ProxyConnectionSettings {
    ProxyConnectionSettings {
      id: id.to_owned(),
      name: id.to_uppercase(),
      domain: format!("{id}.test"),
      listen_host: Ipv4Addr::LOCALHOST.to_string(),
      listen_port: port,
      target,
      allowed_origins: vec![format!("http://{id}.test")],
      upstream_response_timeout_seconds: DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS,
    }
  }

  #[tokio::test]
  async fn adds_and_removes_a_persistent_connection_safely() {
    let port = reserve_port();
    let mut manager = ProxyManager::new(Vec::new()).expect("create Proxy Manager");
    let id = manager
      .add(ProxyConnectionInput {
        id: " Custom-Api ".to_owned(),
        domain: "Custom-Api.test.".to_owned(),
        listen_port: port,
        target: "http://127.0.0.1:9/".to_owned(),
        allowed_origins: Vec::new(),
        upstream_response_timeout_seconds: None,
      })
      .expect("add Proxy connection");

    assert_eq!(id, "custom-api");
    let settings = manager.connections();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].domain, "custom-api.test");
    assert_eq!(settings[0].target, "http://127.0.0.1:9");
    assert_eq!(
      settings[0].upstream_response_timeout_seconds,
      DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS
    );
    assert_eq!(
      settings[0].allowed_origins,
      vec![
        "http://custom-api.test".to_owned(),
        "https://custom-api.test".to_owned()
      ]
    );

    manager.start(&id).await.expect("start Proxy connection");
    manager.remove(&id).await.expect("remove Proxy connection");
    assert!(manager.connections().is_empty());
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("removed Proxy port should be free");
  }

  #[tokio::test]
  async fn updates_a_running_connection_and_preserves_credential_origins() {
    let original_port = reserve_port();
    let updated_port = reserve_port();
    let mut manager = ProxyManager::new(Vec::new()).expect("create Proxy Manager");
    let id = manager
      .add(ProxyConnectionInput {
        id: "example-edit".to_owned(),
        domain: "example-edit.test".to_owned(),
        listen_port: original_port,
        target: "http://127.0.0.1:9".to_owned(),
        allowed_origins: vec!["http://example-edit.test:8100".to_owned()],
        upstream_response_timeout_seconds: None,
      })
      .expect("add Proxy connection");
    manager.start(&id).await.expect("start Proxy connection");

    let (previous, was_running) = manager
      .update(
        &id,
        ProxyConnectionInput {
          id: id.clone(),
          domain: "example-edit.test".to_owned(),
          listen_port: updated_port,
          target: "http://127.0.0.1:10/".to_owned(),
          allowed_origins: vec![
            "http://example-edit.test:8100/".to_owned(),
            "http://example-edit.test:8100".to_owned(),
          ],
          upstream_response_timeout_seconds: Some(300),
        },
      )
      .await
      .expect("update running Proxy connection");

    assert!(was_running);
    assert_eq!(previous.listen_port, original_port);
    let settings = manager.connections();
    assert_eq!(settings[0].listen_port, updated_port);
    assert_eq!(settings[0].target, "http://127.0.0.1:10");
    assert_eq!(settings[0].upstream_response_timeout_seconds, 300);
    assert_eq!(
      settings[0].allowed_origins,
      vec!["http://example-edit.test:8100".to_owned()]
    );
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, original_port))
      .expect("updated Proxy should release its original port");
    assert!(StdTcpListener::bind((Ipv4Addr::LOCALHOST, updated_port)).is_err());

    manager.stop(&id).await.expect("stop updated Proxy");
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, updated_port))
      .expect("stopped updated Proxy should release its port");
  }

  #[test]
  fn normalizes_zero_timeout_and_accepts_maximum_timeout() {
    let mut persisted =
      test_connection("persisted-timeout", 31_099, "http://127.0.0.1:9".to_owned());
    persisted.upstream_response_timeout_seconds = 0;
    let persisted_manager =
      ProxyManager::new(vec![persisted]).expect("normalize a persisted zero timeout");
    assert_eq!(
      persisted_manager.connections()[0].upstream_response_timeout_seconds,
      DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS
    );

    let mut manager = ProxyManager::new(Vec::new()).expect("create Proxy Manager");
    manager
      .add(ProxyConnectionInput {
        id: "default-timeout".to_owned(),
        domain: "default-timeout.test".to_owned(),
        listen_port: 31_100,
        target: "http://127.0.0.1:9".to_owned(),
        allowed_origins: Vec::new(),
        upstream_response_timeout_seconds: Some(0),
      })
      .expect("accept zero as the default timeout");
    manager
      .add(ProxyConnectionInput {
        id: "maximum-timeout".to_owned(),
        domain: "maximum-timeout.test".to_owned(),
        listen_port: 31_101,
        target: "http://127.0.0.1:9".to_owned(),
        allowed_origins: Vec::new(),
        upstream_response_timeout_seconds: Some(360),
      })
      .expect("accept the maximum timeout");

    let settings = manager.connections();
    assert_eq!(
      settings[0].upstream_response_timeout_seconds,
      DEFAULT_PROXY_UPSTREAM_RESPONSE_TIMEOUT_SECONDS
    );
    assert_eq!(settings[1].upstream_response_timeout_seconds, 360);
  }

  #[test]
  fn rejects_invalid_or_conflicting_custom_connections() {
    let port = reserve_port();
    let mut manager = ProxyManager::new(Vec::new()).expect("create Proxy Manager");
    manager
      .add(ProxyConnectionInput {
        id: "custom".to_owned(),
        domain: "custom.test".to_owned(),
        listen_port: port,
        target: "http://127.0.0.1:9".to_owned(),
        allowed_origins: Vec::new(),
        upstream_response_timeout_seconds: None,
      })
      .expect("add first Proxy connection");

    for (input, expected) in [
      (
        ProxyConnectionInput {
          id: "custom".to_owned(),
          domain: "other.test".to_owned(),
          listen_port: reserve_port(),
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: None,
        },
        "duplicate Proxy connection id",
      ),
      (
        ProxyConnectionInput {
          id: "other".to_owned(),
          domain: "other.test".to_owned(),
          listen_port: port,
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: None,
        },
        "both use port",
      ),
      (
        ProxyConnectionInput {
          id: "same-domain".to_owned(),
          domain: "custom.test".to_owned(),
          listen_port: reserve_port(),
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: None,
        },
        "both use domain",
      ),
      (
        ProxyConnectionInput {
          id: "invalid".to_owned(),
          domain: "example.com".to_owned(),
          listen_port: reserve_port(),
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: None,
        },
        "invalid Proxy domain",
      ),
      (
        ProxyConnectionInput {
          id: "secure".to_owned(),
          domain: "secure.test".to_owned(),
          listen_port: reserve_port(),
          target: "https://api.example.test".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: None,
        },
        "absolute HTTP URL",
      ),
      (
        ProxyConnectionInput {
          id: "origin".to_owned(),
          domain: "origin.test".to_owned(),
          listen_port: reserve_port(),
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: vec!["*".to_owned()],
          upstream_response_timeout_seconds: None,
        },
        "must use http or https",
      ),
      (
        ProxyConnectionInput {
          id: "timeout".to_owned(),
          domain: "timeout.test".to_owned(),
          listen_port: reserve_port(),
          target: "http://127.0.0.1:9".to_owned(),
          allowed_origins: Vec::new(),
          upstream_response_timeout_seconds: Some(361),
        },
        "between 1 and 360 seconds",
      ),
    ] {
      let error = manager.add(input).expect_err("reject Proxy connection");
      assert!(error.to_string().contains(expected), "{error:#}");
    }
  }

  #[tokio::test]
  async fn requires_repeated_health_results_before_state_transitions() {
    let settings = test_connection("stable", reserve_port(), "http://127.0.0.1:9".to_owned());
    let health = ConnectionHealth::new(&settings);
    let failure = || UpstreamHealthFailure {
      message: "Proxy upstream health check failed: refused".to_owned(),
      error_kind: "connection_refused",
      os_error_code: Some(10061),
    };

    health
      .record_periodic_failure(failure(), Duration::from_millis(10))
      .await;
    health
      .record_periodic_failure(failure(), Duration::from_millis(10))
      .await;
    assert_eq!(health.last_error().await, None);

    health
      .record_periodic_failure(failure(), Duration::from_millis(10))
      .await;
    assert_eq!(
      health.last_error().await.as_deref(),
      Some("Proxy upstream health check failed: refused")
    );

    health
      .record_periodic_success(Duration::from_millis(5))
      .await;
    assert!(health.last_error().await.is_some());
    health
      .record_periodic_success(Duration::from_millis(5))
      .await;
    assert_eq!(health.last_error().await, None);
  }

  #[test]
  fn staggers_health_checks_and_masks_target_credentials() {
    let first = health_initial_delay("first");
    let second = health_initial_delay("second");
    assert!(first > Duration::ZERO && first <= UPSTREAM_HEALTH_INTERVAL);
    assert!(second > Duration::ZERO && second <= UPSTREAM_HEALTH_INTERVAL);
    assert_ne!(first, second);
    assert_eq!(
      masked_target_authority("http://user:secret@example.test:8080/path"),
      "example.test:8080"
    );
  }

  #[tokio::test]
  async fn proxies_requests_with_host_rewrite_and_cors_then_releases_port() {
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
      .await
      .expect("start upstream");
    let upstream_address = upstream.local_addr().expect("upstream address");
    let upstream_task = tokio::spawn(async move {
      let (mut stream, _) = upstream.accept().await.expect("accept upstream request");
      let mut request = Vec::new();
      let mut buffer = [0_u8; 1024];
      loop {
        let read = stream
          .read(&mut buffer)
          .await
          .expect("read upstream request");
        if read == 0 {
          break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
          break;
        }
      }
      let request = String::from_utf8(request).expect("UTF-8 request");
      assert!(request.starts_with("GET /api/ping?source=test HTTP/1.1"));
      assert!(request
        .to_ascii_lowercase()
        .contains(&format!("host: {upstream_address}").to_ascii_lowercase()));
      let request_lower = request.to_ascii_lowercase();
      assert!(request_lower.contains("authorization: bearer test-token"));
      assert!(request_lower.contains("cookie: laravel_session=test-session"));
      stream
        .write_all(
          b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nSet-Cookie: laravel_session=updated; Path=/; HttpOnly\r\nConnection: close\r\n\r\nOK",
        )
        .await
        .expect("write upstream response");
    });

    let proxy_port = reserve_port();
    let connection = test_connection("test", proxy_port, format!("http://{upstream_address}"));
    let mut manager = ProxyManager::new(vec![connection]).expect("create Proxy Manager");
    manager.start("test").await.expect("start Proxy");

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
      .await
      .expect("connect to Proxy");
    client
      .write_all(
        b"GET /api/ping?source=test HTTP/1.1\r\nHost: test.test:3000\r\nOrigin: http://test.test\r\nAuthorization: Bearer test-token\r\nCookie: laravel_session=test-session\r\nConnection: close\r\n\r\n",
      )
      .await
      .expect("write Proxy request");
    let mut response = Vec::new();
    client
      .read_to_end(&mut response)
      .await
      .expect("read Proxy response");
    let response = String::from_utf8(response).expect("UTF-8 response");
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response
      .to_ascii_lowercase()
      .contains("access-control-allow-origin: http://test.test"));
    assert!(response
      .to_ascii_lowercase()
      .contains("access-control-allow-credentials: true"));
    assert!(response
      .to_ascii_lowercase()
      .contains("set-cookie: laravel_session=updated; path=/; httponly"));
    assert!(response.ends_with("OK"));
    upstream_task.await.expect("complete upstream task");

    manager.stop("test").await.expect("stop Proxy");
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, proxy_port)).expect("Proxy port should be released");
  }

  #[tokio::test]
  async fn applies_response_timeout_without_interrupting_a_streaming_body() {
    let delayed_upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
      .await
      .expect("start delayed upstream");
    let delayed_address = delayed_upstream
      .local_addr()
      .expect("delayed upstream address");
    let delayed_task = tokio::spawn(async move {
      let (mut stream, _) = delayed_upstream
        .accept()
        .await
        .expect("accept delayed request");
      let mut request = [0_u8; 1024];
      let _ = stream.read(&mut request).await;
      tokio::time::sleep(Duration::from_millis(1_100)).await;
      let _ = stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
        .await;
    });
    let delayed_port = reserve_port();
    let mut delayed_connection = test_connection(
      "delayed-response",
      delayed_port,
      format!("http://{delayed_address}"),
    );
    delayed_connection.upstream_response_timeout_seconds = 1;
    let mut delayed_manager =
      ProxyManager::new(vec![delayed_connection]).expect("create delayed Proxy Manager");
    delayed_manager
      .start("delayed-response")
      .await
      .expect("start delayed Proxy");
    let mut delayed_client = TcpStream::connect((Ipv4Addr::LOCALHOST, delayed_port))
      .await
      .expect("connect to delayed Proxy");
    delayed_client
      .write_all(b"GET / HTTP/1.1\r\nHost: delayed-response.test\r\nConnection: close\r\n\r\n")
      .await
      .expect("write delayed Proxy request");
    let mut delayed_response = Vec::new();
    delayed_client
      .read_to_end(&mut delayed_response)
      .await
      .expect("read delayed Proxy response");
    assert!(String::from_utf8(delayed_response)
      .expect("UTF-8 delayed response")
      .starts_with("HTTP/1.1 504 Gateway Timeout"));
    delayed_manager
      .stop("delayed-response")
      .await
      .expect("stop delayed Proxy");
    delayed_task.await.expect("complete delayed upstream");

    let streaming_upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
      .await
      .expect("start streaming upstream");
    let streaming_address = streaming_upstream
      .local_addr()
      .expect("streaming upstream address");
    let streaming_task = tokio::spawn(async move {
      let (mut stream, _) = streaming_upstream
        .accept()
        .await
        .expect("accept streaming request");
      let mut request = [0_u8; 1024];
      let _ = stream.read(&mut request).await;
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n")
        .await
        .expect("write streaming headers");
      stream.flush().await.expect("flush streaming headers");
      tokio::time::sleep(Duration::from_millis(1_100)).await;
      stream.write_all(b"OK").await.expect("write streaming body");
    });
    let streaming_port = reserve_port();
    let mut streaming_connection = test_connection(
      "streaming-response",
      streaming_port,
      format!("http://{streaming_address}"),
    );
    streaming_connection.upstream_response_timeout_seconds = 1;
    let mut streaming_manager =
      ProxyManager::new(vec![streaming_connection]).expect("create streaming Proxy Manager");
    streaming_manager
      .start("streaming-response")
      .await
      .expect("start streaming Proxy");
    let mut streaming_client = TcpStream::connect((Ipv4Addr::LOCALHOST, streaming_port))
      .await
      .expect("connect to streaming Proxy");
    streaming_client
      .write_all(b"GET / HTTP/1.1\r\nHost: streaming-response.test\r\nConnection: close\r\n\r\n")
      .await
      .expect("write streaming Proxy request");
    let mut streaming_response = Vec::new();
    streaming_client
      .read_to_end(&mut streaming_response)
      .await
      .expect("read streaming Proxy response");
    let streaming_response =
      String::from_utf8(streaming_response).expect("UTF-8 streaming response");
    assert!(streaming_response.starts_with("HTTP/1.1 200 OK"));
    assert!(streaming_response.ends_with("OK"));
    streaming_task.await.expect("complete streaming upstream");
    streaming_manager
      .stop("streaming-response")
      .await
      .expect("stop streaming Proxy");
  }

  #[tokio::test]
  async fn isolates_a_port_collision_from_other_proxy_connections() {
    let occupied = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("occupy local port");
    let occupied_port = occupied.local_addr().expect("occupied address").port();
    let available_port = reserve_port();
    let mut manager = ProxyManager::new(vec![
      test_connection("blocked", occupied_port, "http://127.0.0.1:9".to_owned()),
      test_connection("healthy", available_port, "http://127.0.0.1:9".to_owned()),
    ])
    .expect("create Proxy Manager");

    let state = manager.start_all().await;
    let blocked = state
      .connections
      .iter()
      .find(|connection| connection.id == "blocked")
      .expect("blocked connection");
    let healthy = state
      .connections
      .iter()
      .find(|connection| connection.id == "healthy")
      .expect("healthy connection");
    assert_eq!(blocked.state, ProxyConnectionState::Failed);
    assert_eq!(healthy.state, ProxyConnectionState::Running);

    manager.stop_all().await;
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, available_port))
      .expect("healthy Proxy port should be released");
  }
}
