use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use fabdev_core::{
  normalize_domain, ProxyConnectionInfo, ProxyConnectionInput, ProxyConnectionSettings,
  ProxyConnectionState, ProxyManagerState,
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

type ProxyBody = BoxBody<Bytes, hyper::Error>;
type ProxyClient = Client<HttpConnector, Incoming>;

struct ConnectionHealth {
  last_error: Mutex<Option<String>>,
}

impl ConnectionHealth {
  fn new() -> Self {
    Self {
      last_error: Mutex::new(None),
    }
  }

  async fn set_error(&self, error: impl Into<String>) {
    *self.last_error.lock().await = Some(error.into());
  }

  async fn clear_error(&self) {
    self.last_error.lock().await.take();
  }

  async fn last_error(&self) -> Option<String> {
    self.last_error.lock().await.clone()
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
    settings: ProxyConnectionSettings,
    should_run: bool,
  ) -> Result<()> {
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

  fn add_settings(&mut self, settings: ProxyConnectionSettings) -> Result<()> {
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
    let health = Arc::new(ConnectionHealth::new());
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
  Ok(settings)
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

async fn run_proxy(
  listener: TcpListener,
  settings: ProxyConnectionSettings,
  health: Arc<ConnectionHealth>,
  mut stop: oneshot::Receiver<()>,
) {
  let mut connector = HttpConnector::new();
  connector.enforce_http(true);
  let client: ProxyClient = Client::builder(TokioExecutor::new()).build(connector);
  let mut connections = JoinSet::new();
  let mut health_interval = tokio::time::interval_at(
    tokio::time::Instant::now() + UPSTREAM_HEALTH_INTERVAL,
    UPSTREAM_HEALTH_INTERVAL,
  );
  health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

  loop {
    tokio::select! {
      _ = &mut stop => break,
      _ = health_interval.tick() => {
        match check_upstream(&settings).await {
          Ok(()) => health.clear_error().await,
          Err(error) => health.set_error(error.to_string()).await,
        }
      }
      accepted = listener.accept() => match accepted {
        Ok((stream, _)) => {
          let client = client.clone();
          let settings = settings.clone();
          let health = Arc::clone(&health);
          connections.spawn(async move {
            serve_client(stream, client, settings, health).await;
          });
        }
        Err(error) => {
          health.set_error(format!("unable to accept Proxy connection: {error}")).await;
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

async fn check_upstream(settings: &ProxyConnectionSettings) -> Result<()> {
  let target: Uri = settings
    .target
    .parse()
    .with_context(|| format!("invalid Proxy target: {}", settings.target))?;
  let authority = target
    .authority()
    .with_context(|| format!("Proxy target has no authority: {}", settings.target))?;
  let host = authority.host();
  let port = authority.port_u16().unwrap_or(80);
  match tokio::time::timeout(UPSTREAM_HEALTH_TIMEOUT, TcpStream::connect((host, port))).await {
    Ok(Ok(_)) => Ok(()),
    Ok(Err(error)) => bail!("Proxy upstream health check failed: {error}"),
    Err(_) => bail!(
      "Proxy upstream health check timed out after {} seconds",
      UPSTREAM_HEALTH_TIMEOUT.as_secs()
    ),
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
      health.set_error(message.clone()).await;
      return Ok(error_response(StatusCode::BAD_GATEWAY, &message));
    }
  };
  let Some(authority) = target_uri.authority().cloned() else {
    let message = "Proxy target URI has no authority".to_owned();
    health.set_error(message.clone()).await;
    return Ok(error_response(StatusCode::BAD_GATEWAY, &message));
  };
  *request.uri_mut() = target_uri;
  if let Ok(host) = authority.as_str().parse() {
    request.headers_mut().insert(HOST, host);
  }

  let upstream = tokio::time::timeout(UPSTREAM_CONNECT_TIMEOUT, client.request(request)).await;
  let mut response = match upstream {
    Ok(Ok(response)) => {
      health.clear_error().await;
      response.map(|body| body.boxed())
    }
    Ok(Err(error)) => {
      let message = format!("Proxy upstream request failed: {error}");
      health.set_error(message.clone()).await;
      error_response(StatusCode::BAD_GATEWAY, &message)
    }
    Err(_) => {
      let message = format!(
        "Proxy upstream timed out after {} seconds",
        UPSTREAM_CONNECT_TIMEOUT.as_secs()
      );
      health.set_error(message.clone()).await;
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
      })
      .expect("add Proxy connection");

    assert_eq!(id, "custom-api");
    let settings = manager.connections();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].domain, "custom-api.test");
    assert_eq!(settings[0].target, "http://127.0.0.1:9");
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
        },
      )
      .await
      .expect("update running Proxy connection");

    assert!(was_running);
    assert_eq!(previous.listen_port, original_port);
    let settings = manager.connections();
    assert_eq!(settings[0].listen_port, updated_port);
    assert_eq!(settings[0].target, "http://127.0.0.1:10");
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
        },
        "must use http or https",
      ),
    ] {
      let error = manager.add(input).expect_err("reject Proxy connection");
      assert!(error.to_string().contains(expected), "{error:#}");
    }
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
