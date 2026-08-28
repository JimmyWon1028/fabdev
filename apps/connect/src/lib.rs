use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream};
use tokio::sync::oneshot;

pub const MANAGED_START: &str = "# BEGIN FABDEV CONNECT";
pub const MANAGED_END: &str = "# END FABDEV CONNECT";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnectSettings {
  pub server: String,
  pub domains: String,
}

impl Default for ConnectSettings {
  fn default() -> Self {
    Self {
      server: "192.168.1.10:18080".to_owned(),
      domains: "site-one.test, site-two.test".to_owned(),
    }
  }
}

pub fn decode_settings(contents: &[u8]) -> Result<ConnectSettings> {
  serde_json::from_slice(contents).context("fabDev Connect 設定格式無效")
}

pub fn encode_settings(settings: &ConnectSettings) -> Result<Vec<u8>> {
  serde_json::to_vec_pretty(settings).context("無法產生 fabDev Connect 設定")
}

pub struct ClientProxy {
  local: SocketAddr,
  shutdown: Option<oneshot::Sender<()>>,
  thread: Option<JoinHandle<()>>,
}

impl ClientProxy {
  pub fn start(remote: SocketAddr) -> Result<Self> {
    Self::start_at("127.0.0.1:80".parse().unwrap(), remote)
  }

  pub fn start_at(local: SocketAddr, remote: SocketAddr) -> Result<Self> {
    if !local.ip().is_loopback() {
      bail!("fabDev Connect 只能監聽本機 loopback 位址");
    }
    TcpStream::connect_timeout(&remote, Duration::from_secs(3))
      .with_context(|| format!("無法連線到 fabDev 主機 {remote}"))?;
    let listener = TcpListener::bind(local)
      .with_context(|| format!("無法使用本機 {local}；請確認 IIS 或其他 Web Server 沒有占用"))?;
    let local = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let (shutdown, shutdown_receiver) = oneshot::channel();
    let thread = thread::spawn(move || {
      let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
      {
        Ok(runtime) => runtime,
        Err(error) => {
          eprintln!("fabDev Connect runtime failed: {error}");
          return;
        }
      };
      if let Err(error) = runtime.block_on(run_proxy(listener, remote, shutdown_receiver)) {
        eprintln!("fabDev Connect listener failed: {error:#}");
      }
    });
    Ok(Self {
      local,
      shutdown: Some(shutdown),
      thread: Some(thread),
    })
  }

  pub fn local_addr(&self) -> SocketAddr {
    self.local
  }

  pub fn stop(&mut self) {
    if let Some(shutdown) = self.shutdown.take() {
      let _ = shutdown.send(());
    }
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

impl Drop for ClientProxy {
  fn drop(&mut self) {
    self.stop();
  }
}

async fn run_proxy(
  listener: TcpListener,
  remote: SocketAddr,
  mut shutdown: oneshot::Receiver<()>,
) -> Result<()> {
  let listener = TokioTcpListener::from_std(listener).context("無法啟動本機非同步 listener")?;
  loop {
    tokio::select! {
      _ = &mut shutdown => break,
      accepted = listener.accept() => {
        let (client, _) = accepted.context("接受本機瀏覽器連線失敗")?;
        tokio::spawn(async move {
          if let Err(error) = proxy_connection(client, remote).await {
            eprintln!("fabDev Connect proxy failed: {error:#}");
          }
        });
      }
    }
  }
  Ok(())
}

async fn proxy_connection(mut client: TokioTcpStream, remote: SocketAddr) -> Result<()> {
  let mut server = TokioTcpStream::connect(remote)
    .await
    .with_context(|| format!("無法連線到 fabDev 主機 {remote}"))?;
  client.set_nodelay(true)?;
  server.set_nodelay(true)?;
  copy_bidirectional(&mut client, &mut server)
    .await
    .context("轉送瀏覽器連線失敗")?;
  Ok(())
}

pub fn validate_domain(domain: &str) -> Result<String> {
  let domain = domain.trim().to_ascii_lowercase();
  let valid = domain.ends_with(".test")
    && domain.len() <= 253
    && domain.bytes().all(|byte| {
      byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
    })
    && domain
      .split('.')
      .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'));
  if !valid {
    bail!("Site 必須是有效的 .test 網域");
  }
  Ok(domain)
}

pub fn parse_domains(value: &str) -> Result<Vec<String>> {
  let mut domains = Vec::new();
  for value in
    value.split(|character: char| character.is_ascii_whitespace() || matches!(character, ',' | ';'))
  {
    if value.is_empty() {
      continue;
    }
    let domain = validate_domain(value)?;
    if !domains.contains(&domain) {
      domains.push(domain);
    }
  }
  if domains.is_empty() {
    bail!("請至少輸入一個 .test Site");
  }
  Ok(domains)
}

pub fn update_hosts_contents(existing: &str, domains: Option<&[String]>) -> Result<String> {
  let without_managed = remove_managed_block(existing)?;
  if let Some(domains) = domains {
    if domains.is_empty() {
      bail!("請至少輸入一個 .test Site");
    }
    let domains = domains
      .iter()
      .map(|domain| validate_domain(domain))
      .collect::<Result<Vec<_>>>()?;
    for domain in &domains {
      if contains_unmanaged_domain(&without_managed, domain) {
        bail!("hosts 已有 {domain} 設定，請先移除原有紀錄");
      }
    }
    let mut contents = without_managed.trim_end_matches(['\r', '\n']).to_owned();
    contents.push_str("\r\n\r\n");
    contents.push_str(MANAGED_START);
    for domain in domains {
      contents.push_str("\r\n127.0.0.1 ");
      contents.push_str(&domain);
    }
    contents.push_str("\r\n");
    contents.push_str(MANAGED_END);
    contents.push_str("\r\n");
    return Ok(contents);
  }
  let mut contents = without_managed.trim_end_matches(['\r', '\n']).to_owned();
  contents.push_str("\r\n");
  Ok(contents)
}

fn remove_managed_block(contents: &str) -> Result<String> {
  let Some(start) = contents.find(MANAGED_START) else {
    if contents.contains(MANAGED_END) {
      bail!("hosts 含有不完整的 fabDev Connect 管理區塊");
    }
    return Ok(contents.to_owned());
  };
  let remainder = &contents[start + MANAGED_START.len()..];
  let end_offset = remainder
    .find(MANAGED_END)
    .context("hosts 含有不完整的 fabDev Connect 管理區塊")?;
  let end = start + MANAGED_START.len() + end_offset + MANAGED_END.len();
  if contents[end..].contains(MANAGED_START) || contents[end..].contains(MANAGED_END) {
    bail!("hosts 含有多個 fabDev Connect 管理區塊");
  }
  let mut output = contents[..start].trim_end_matches(['\r', '\n']).to_owned();
  output.push_str(&contents[end..]);
  Ok(output)
}

fn contains_unmanaged_domain(contents: &str, domain: &str) -> bool {
  contents.lines().any(|line| {
    let values = line.split('#').next().unwrap_or_default();
    values
      .split_ascii_whitespace()
      .skip(1)
      .any(|value| value.eq_ignore_ascii_case(domain))
  })
}

#[cfg(test)]
mod tests {
  use std::io::{Read, Write};

  use super::*;

  #[test]
  fn forwards_tcp_traffic_and_releases_the_local_port() {
    let upstream = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
    let upstream_addr = upstream.local_addr().unwrap();
    let upstream_task = thread::spawn(move || {
      let _ = upstream.accept().expect("accept preflight");
      let (mut stream, _) = upstream.accept().expect("accept proxy");
      let mut request = [0_u8; 4];
      stream.read_exact(&mut request).expect("read request");
      assert_eq!(&request, b"ping");
      stream.write_all(b"pong").expect("write response");
    });

    let mut proxy = ClientProxy::start_at("127.0.0.1:0".parse().unwrap(), upstream_addr)
      .expect("start client proxy");
    let local = proxy.local_addr();
    let mut client = TcpStream::connect(local).expect("connect client proxy");
    client.write_all(b"ping").expect("write request");
    let mut response = [0_u8; 4];
    client.read_exact(&mut response).expect("read response");
    assert_eq!(&response, b"pong");
    drop(client);
    proxy.stop();
    upstream_task.join().expect("join upstream");
    TcpListener::bind(local).expect("local port released");
  }

  #[test]
  fn forwards_concurrent_browser_connections() {
    const CONNECTIONS: usize = 16;
    let upstream = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
    let upstream_addr = upstream.local_addr().unwrap();
    let upstream_task = thread::spawn(move || {
      let _ = upstream.accept().expect("accept preflight");
      let mut workers = Vec::new();
      for _ in 0..CONNECTIONS {
        let (mut stream, _) = upstream.accept().expect("accept proxy");
        workers.push(thread::spawn(move || {
          let mut request = [0_u8; 4];
          stream.read_exact(&mut request).expect("read request");
          assert_eq!(&request, b"ping");
          thread::sleep(Duration::from_millis(25));
          stream.write_all(b"pong").expect("write response");
        }));
      }
      for worker in workers {
        worker.join().expect("join upstream worker");
      }
    });

    let mut proxy = ClientProxy::start_at("127.0.0.1:0".parse().unwrap(), upstream_addr)
      .expect("start client proxy");
    let local = proxy.local_addr();
    let mut clients = Vec::new();
    for _ in 0..CONNECTIONS {
      clients.push(thread::spawn(move || {
        let mut client = TcpStream::connect(local).expect("connect client proxy");
        client
          .set_read_timeout(Some(Duration::from_secs(3)))
          .expect("set client timeout");
        client.write_all(b"ping").expect("write request");
        let mut response = [0_u8; 4];
        client.read_exact(&mut response).expect("read response");
        assert_eq!(&response, b"pong");
      }));
    }
    for client in clients {
      client.join().expect("join client");
    }
    proxy.stop();
    upstream_task.join().expect("join upstream");
  }

  #[test]
  fn adds_and_removes_only_the_connect_hosts_block() {
    let existing = "127.0.0.1 localhost\r\n10.0.0.2 intranet\r\n";
    let domains = vec!["site-two.test".to_owned(), "site-one.test".to_owned()];
    let connected = update_hosts_contents(existing, Some(&domains)).expect("add domains");
    assert!(connected.contains("127.0.0.1 site-two.test"));
    assert!(connected.contains("127.0.0.1 site-one.test"));
    assert!(connected.contains("10.0.0.2 intranet"));

    let disconnected = update_hosts_contents(&connected, None).expect("remove domain");
    assert!(!disconnected.contains("site-two.test"));
    assert!(!disconnected.contains("site-one.test"));
    assert!(disconnected.contains("127.0.0.1 localhost"));
    assert!(disconnected.contains("10.0.0.2 intranet"));
  }

  #[test]
  fn rejects_an_existing_unmanaged_domain() {
    let existing = "192.168.1.10 site-two.test\r\n";
    let domains = vec!["site-two.test".to_owned(), "site-one.test".to_owned()];
    let error = update_hosts_contents(existing, Some(&domains)).expect_err("reject domain");
    assert!(error.to_string().contains("已有 site-two.test"));
  }

  #[test]
  fn validates_only_test_domains() {
    assert_eq!(validate_domain("SITE-TWO.test").unwrap(), "site-two.test");
    assert!(validate_domain("site-two.example.com").is_err());
    assert!(validate_domain("-site-two.test").is_err());
  }

  #[test]
  fn parses_multiple_unique_domains() {
    assert_eq!(
      parse_domains("SITE-ONE.test, site-two.test; site-one.test demo.test").unwrap(),
      vec!["site-one.test", "site-two.test", "demo.test"]
    );
    assert!(parse_domains("  , ; ").is_err());
  }

  #[test]
  fn persists_connect_settings_and_defaults_missing_fields() {
    let settings = ConnectSettings {
      server: "192.0.2.10:18080".to_owned(),
      domains: "site-one.test, demo.test".to_owned(),
    };
    assert_eq!(
      decode_settings(&encode_settings(&settings).unwrap()).unwrap(),
      settings
    );
    assert_eq!(
      decode_settings(br#"{"server":"10.0.0.2:18080"}"#)
        .unwrap()
        .domains,
      ConnectSettings::default().domains
    );
  }
}
