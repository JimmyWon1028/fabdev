use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, RwLock};
use tokio::task::{JoinHandle, JoinSet};

mod restricted_http;

const MAX_HTTP_HEADER_SIZE: usize = 64 * 1024;

pub struct ShareServer {
  local_addr: SocketAddr,
  allowed_domains: Arc<RwLock<Option<HashSet<String>>>>,
  shutdown: Option<oneshot::Sender<()>>,
  task: Option<JoinHandle<Result<()>>>,
}

impl ShareServer {
  pub async fn start(listen: SocketAddr, upstream: SocketAddr) -> Result<Self> {
    Self::start_with_domains(listen, upstream, None).await
  }

  pub async fn start_restricted(
    listen: SocketAddr,
    upstream: SocketAddr,
    domains: Vec<String>,
  ) -> Result<Self> {
    if domains.is_empty() {
      bail!("fabDev Share requires at least one allowed domain");
    }
    Self::start_with_domains(listen, upstream, Some(domains)).await
  }

  async fn start_with_domains(
    listen: SocketAddr,
    upstream: SocketAddr,
    domains: Option<Vec<String>>,
  ) -> Result<Self> {
    if !upstream.ip().is_loopback() {
      bail!("fabDev Share upstream must use loopback");
    }
    let listener = TcpListener::bind(listen)
      .await
      .with_context(|| format!("unable to listen for LAN Site Share at {listen}"))?;
    let local_addr = listener.local_addr()?;
    let allowed_domains = Arc::new(RwLock::new(domains.map(normalize_domains)));
    let (shutdown, receiver) = oneshot::channel();
    let task = tokio::spawn(run(
      listener,
      upstream,
      Arc::clone(&allowed_domains),
      receiver,
    ));
    Ok(Self {
      local_addr,
      allowed_domains,
      shutdown: Some(shutdown),
      task: Some(task),
    })
  }

  pub fn local_addr(&self) -> SocketAddr {
    self.local_addr
  }

  pub async fn set_allowed_domains(&self, domains: Vec<String>) -> Result<()> {
    if domains.is_empty() {
      bail!("fabDev Share requires at least one allowed domain");
    }
    *self.allowed_domains.write().await = Some(normalize_domains(domains));
    Ok(())
  }

  pub async fn stop(&mut self) -> Result<()> {
    if let Some(shutdown) = self.shutdown.take() {
      let _ = shutdown.send(());
    }
    if let Some(task) = self.task.take() {
      task.await.context("LAN Site Share task failed")??;
    }
    Ok(())
  }
}

impl Drop for ShareServer {
  fn drop(&mut self) {
    if let Some(shutdown) = self.shutdown.take() {
      let _ = shutdown.send(());
    }
    if let Some(task) = self.task.take() {
      task.abort();
    }
  }
}

async fn run(
  listener: TcpListener,
  upstream: SocketAddr,
  allowed_domains: Arc<RwLock<Option<HashSet<String>>>>,
  mut shutdown: oneshot::Receiver<()>,
) -> Result<()> {
  let mut connections = JoinSet::new();
  loop {
    tokio::select! {
      biased;
      _ = &mut shutdown => {
        connections.shutdown().await;
        return Ok(());
      }
      _ = connections.join_next(), if !connections.is_empty() => {}
      accepted = listener.accept() => {
        let (client, _) = accepted.context("unable to accept LAN Site Share connection")?;
        let allowed_domains = Arc::clone(&allowed_domains);
        connections.spawn(async move {
          if let Err(error) = proxy(client, upstream, allowed_domains).await {
            eprintln!("fabDev Share connection failed: {error:#}");
          }
        });
      }
    }
  }
}

async fn proxy(
  mut client: TcpStream,
  upstream: SocketAddr,
  allowed_domains: Arc<RwLock<Option<HashSet<String>>>>,
) -> Result<()> {
  let initial_domains = allowed_domains.read().await.clone();
  let initial_request = match initial_domains.as_ref() {
    Some(domains) => match read_allowed_request(&mut client, domains).await? {
      Some(request) => Some(request),
      None => {
        client
          .write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
          .await?;
        return Ok(());
      }
    },
    None => None,
  };
  let mut server = TcpStream::connect(upstream)
    .await
    .with_context(|| format!("unable to connect to fabDev Nginx at {upstream}"))?;
  if let Some(request) = initial_request {
    return restricted_http::proxy(client, server, request, allowed_domains).await;
  }
  copy_bidirectional(&mut client, &mut server)
    .await
    .context("LAN Site Share proxy failed")?;
  Ok(())
}

fn normalize_domains(domains: Vec<String>) -> HashSet<String> {
  domains
    .into_iter()
    .map(|domain| domain.trim().to_ascii_lowercase())
    .collect()
}

async fn read_allowed_request(
  client: &mut TcpStream,
  allowed_domains: &HashSet<String>,
) -> Result<Option<Vec<u8>>> {
  let mut request = Vec::with_capacity(4096);
  loop {
    if request.len() >= MAX_HTTP_HEADER_SIZE {
      bail!("LAN Site Share request headers exceed {MAX_HTTP_HEADER_SIZE} bytes");
    }
    let mut chunk = [0_u8; 4096];
    let read = client.read(&mut chunk).await?;
    if read == 0 {
      bail!("LAN Site Share client closed before sending HTTP headers");
    }
    request.extend_from_slice(&chunk[..read]);
    if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
      let headers = std::str::from_utf8(&request[..end + 4])
        .context("LAN Site Share received invalid HTTP headers")?;
      let domain = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name
          .eq_ignore_ascii_case("host")
          .then(|| value.trim().split(':').next().unwrap_or_default())
      });
      return Ok(
        domain
          .map(|domain| allowed_domains.contains(&domain.to_ascii_lowercase()))
          .unwrap_or(false)
          .then_some(request),
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  use super::*;

  #[tokio::test]
  async fn proxies_bidirectional_tcp_and_releases_the_port() {
    let upstream = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("listen for upstream fixture");
    let upstream_addr = upstream.local_addr().expect("read upstream address");
    let fixture = tokio::spawn(async move {
      let (mut stream, _) = upstream.accept().await.expect("accept fixture connection");
      let mut request = [0_u8; 4];
      stream.read_exact(&mut request).await.expect("read fixture");
      assert_eq!(&request, b"ping");
      stream.write_all(b"pong").await.expect("write fixture");
    });

    let mut share = ShareServer::start("127.0.0.1:0".parse().unwrap(), upstream_addr)
      .await
      .expect("start LAN share");
    let shared_addr = share.local_addr();
    let mut client = TcpStream::connect(shared_addr)
      .await
      .expect("connect to LAN share");
    client
      .write_all(b"ping")
      .await
      .expect("write through share");
    let mut response = [0_u8; 4];
    client
      .read_exact(&mut response)
      .await
      .expect("read through share");
    assert_eq!(&response, b"pong");
    fixture.await.expect("join fixture");

    share.stop().await.expect("stop LAN share");
    TcpListener::bind(shared_addr)
      .await
      .expect("share port should be released");
  }

  #[tokio::test]
  async fn rejects_a_non_loopback_upstream() {
    let error = ShareServer::start(
      "127.0.0.1:0".parse().unwrap(),
      "192.0.2.20:8080".parse().unwrap(),
    )
    .await
    .err()
    .expect("reject non-loopback upstream");
    assert!(error.to_string().contains("upstream must use loopback"));
  }

  #[tokio::test]
  async fn restricts_and_updates_shared_http_domains() {
    let upstream = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("listen for upstream fixture");
    let upstream_addr = upstream.local_addr().expect("read upstream address");
    let fixture = tokio::spawn(async move {
      let (mut stream, _) = upstream.accept().await.expect("accept allowed request");
      let mut request = vec![0_u8; 1024];
      let read = stream
        .read(&mut request)
        .await
        .expect("read allowed request");
      let request = String::from_utf8_lossy(&request[..read]);
      assert!(request.contains("Host: site-one.test"));
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nOK")
        .await
        .expect("write allowed response");
    });

    let mut share = ShareServer::start_restricted(
      "127.0.0.1:0".parse().unwrap(),
      upstream_addr,
      vec!["demo.test".to_owned()],
    )
    .await
    .expect("start restricted LAN share");
    let shared_addr = share.local_addr();
    let mut denied = TcpStream::connect(shared_addr)
      .await
      .expect("connect denied client");
    denied
      .write_all(b"GET / HTTP/1.1\r\nHost: site-one.test\r\n\r\n")
      .await
      .expect("write denied request");
    let mut denied_response = Vec::new();
    denied
      .read_to_end(&mut denied_response)
      .await
      .expect("read denied response");
    assert!(denied_response.starts_with(b"HTTP/1.1 403 Forbidden"));

    share
      .set_allowed_domains(vec!["site-one.test".to_owned()])
      .await
      .expect("update allowed domains");
    let mut allowed = TcpStream::connect(shared_addr)
      .await
      .expect("connect allowed client");
    allowed
      .write_all(b"GET / HTTP/1.1\r\nHost: site-one.test\r\n\r\n")
      .await
      .expect("write allowed request");
    let mut allowed_response = Vec::new();
    allowed
      .read_to_end(&mut allowed_response)
      .await
      .expect("read allowed response");
    assert!(allowed_response.starts_with(b"HTTP/1.1 200 OK"));
    fixture.await.expect("join upstream fixture");
    share.stop().await.expect("stop restricted LAN share");
  }
}
