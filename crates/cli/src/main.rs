use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fabdev_core::{
  create_site, AgentEndpoint, AgentRequest, AgentResponse, AppPaths, PhpVersion,
  ProxyConnectionInput, SiteInput, SiteRepository,
};
use fabdev_runtime::{install_tar_gz, RuntimeRelease};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
#[cfg(unix)]
use tokio::net::UnixStream;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
  name = "fabdev",
  version,
  about = "Manage the local fabDev environment"
)]
struct Arguments {
  #[cfg(unix)]
  #[arg(long)]
  socket: Option<PathBuf>,
  #[cfg(windows)]
  #[arg(long)]
  pipe: Option<String>,
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  Ping,
  Status,
  Sites,
  AddSite {
    path: PathBuf,
    #[arg(long)]
    domain: Option<String>,
    #[arg(long)]
    document_root: Option<PathBuf>,
    #[arg(long, default_value = "8.2")]
    php: PhpVersion,
  },
  RemoveSite {
    id: Uuid,
  },
  SetSitePhp {
    id: Uuid,
    php: PhpVersion,
  },
  Secure {
    id: Uuid,
  },
  Unsecure {
    id: Uuid,
  },
  LanShare,
  Share {
    id: Uuid,
    #[arg(long, default_value_t = 18_080)]
    port: u16,
  },
  Unshare {
    id: Uuid,
  },
  StopShare,
  Runtimes,
  InstallPhpRuntime {
    artifact: PathBuf,
    release: PathBuf,
  },
  SetGlobalPhp {
    version: String,
  },
  RemovePhpRuntime {
    version: String,
  },
  PhpIni {
    php: PhpVersion,
  },
  SavePhpIni {
    php: PhpVersion,
    file: PathBuf,
  },
  NodeRuntime,
  InstallNodeRuntime {
    artifact: PathBuf,
    release: PathBuf,
  },
  RemoveNodeRuntime,
  Proxies,
  AddProxy {
    id: String,
    #[arg(long)]
    domain: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    target: String,
    #[arg(long = "origin")]
    allowed_origins: Vec<String>,
  },
  UpdateProxy {
    id: String,
    #[arg(long)]
    domain: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    target: String,
    #[arg(long = "origin")]
    allowed_origins: Vec<String>,
  },
  RemoveProxy {
    id: String,
  },
  StartProxy {
    id: String,
  },
  StopProxy {
    id: String,
  },
  StartAllProxies,
  StopAllProxies,
  Start,
  Stop,
  StartMariaDb,
  StopMariaDb,
  InstallMariaDbRuntime {
    artifact: PathBuf,
    release: PathBuf,
  },
  RemoveMariaDbRuntime,
  InstallRuntime {
    artifact: PathBuf,
    release: PathBuf,
    #[arg(long)]
    data_dir: Option<PathBuf>,
  },
  #[command(hide = true)]
  SeedDemo {
    path: PathBuf,
    #[arg(long)]
    data_dir: Option<PathBuf>,
  },
}

#[tokio::main]
async fn main() -> Result<()> {
  let arguments = Arguments::parse();
  if let Command::InstallRuntime {
    artifact,
    release,
    data_dir,
  } = &arguments.command
  {
    let release: RuntimeRelease = serde_json::from_reader(
      std::fs::File::open(release).context("unable to open runtime release descriptor")?,
    )
    .context("invalid runtime release descriptor")?;
    let paths = match data_dir {
      Some(path) => AppPaths::from_root(path),
      None => AppPaths::discover().context("unable to locate fabDev application data")?,
    };
    paths.ensure()?;
    let layout = install_tar_gz(
      artifact,
      &release.sha256,
      &release.name,
      &release.version,
      &paths.runtimes,
    )?;
    println!(
      "Installed {} {} at {}",
      release.name,
      release.version,
      layout.runtime_root.display()
    );
    return Ok(());
  }
  if let Command::SeedDemo { path, data_dir } = &arguments.command {
    let paths = match data_dir {
      Some(path) => AppPaths::from_root(path),
      None => AppPaths::discover().context("unable to locate fabDev application data")?,
    };
    if seed_demo(&paths, path)? {
      println!("Created demo.test at {}", path.display());
    } else {
      println!("Existing Sites found; demo.test was not added");
    }
    return Ok(());
  }
  let paths = AppPaths::discover().context("unable to locate fabDev application data")?;
  #[cfg(unix)]
  let endpoint =
    AgentEndpoint::UnixSocket(arguments.socket.unwrap_or_else(|| paths.agent_socket()));
  #[cfg(windows)]
  let endpoint = arguments
    .pipe
    .map(AgentEndpoint::NamedPipe)
    .unwrap_or_else(|| paths.agent_endpoint());
  if let Command::Secure { id } = &arguments.command {
    let ca_response = send_request(&endpoint, AgentRequest::EnsureLocalCa).await?;
    let AgentResponse::LocalCaReady(ca) = ca_response else {
      println!("{}", serde_json::to_string_pretty(&ca_response)?);
      if matches!(ca_response, AgentResponse::Error { .. }) {
        std::process::exit(1);
      }
      anyhow::bail!("fabDev Agent returned an unexpected response");
    };
    trust_local_ca(&ca.certificate_path)?;
    let response = send_request(
      &endpoint,
      AgentRequest::SetSiteHttps {
        site_id: *id,
        secured: true,
      },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    if matches!(response, AgentResponse::Error { .. }) {
      std::process::exit(1);
    }
    return Ok(());
  }
  let request = match arguments.command {
    Command::Ping => AgentRequest::Ping,
    Command::Status => AgentRequest::GetStatus,
    Command::Sites => AgentRequest::ListSites,
    Command::AddSite {
      path,
      domain,
      document_root,
      php,
    } => AgentRequest::AddSite(SiteInput {
      name: None,
      domain,
      project_path: path,
      document_root,
      php_version: Some(php),
    }),
    Command::RemoveSite { id } => AgentRequest::RemoveSite { site_id: id },
    Command::SetSitePhp { id, php } => AgentRequest::SetSitePhp {
      site_id: id,
      php_version: Some(php),
    },
    Command::Secure { .. } => unreachable!("handled before Agent request mapping"),
    Command::Unsecure { id } => AgentRequest::SetSiteHttps {
      site_id: id,
      secured: false,
    },
    Command::LanShare => AgentRequest::GetLanShare,
    Command::Share { id, port } => AgentRequest::StartLanShare { site_id: id, port },
    Command::Unshare { id } => AgentRequest::StopLanShareSite { site_id: id },
    Command::StopShare => AgentRequest::StopLanShare,
    Command::Runtimes => AgentRequest::ListPhpRuntimes,
    Command::InstallPhpRuntime { artifact, release } => AgentRequest::InstallPhpRuntime {
      artifact_path: artifact,
      release_path: release,
    },
    Command::SetGlobalPhp { version } => AgentRequest::SetGlobalPhp { version },
    Command::RemovePhpRuntime { version } => AgentRequest::RemovePhpRuntime { version },
    Command::PhpIni { php } => AgentRequest::GetPhpIni { php_version: php },
    Command::SavePhpIni { php, file } => AgentRequest::SavePhpIni {
      php_version: php,
      contents: std::fs::read_to_string(&file)
        .with_context(|| format!("unable to read php.ini from {}", file.display()))?,
    },
    Command::NodeRuntime => AgentRequest::GetNodeRuntime,
    Command::InstallNodeRuntime { artifact, release } => AgentRequest::InstallNodeRuntime {
      artifact_path: artifact,
      release_path: release,
    },
    Command::RemoveNodeRuntime => AgentRequest::RemoveNodeRuntime,
    Command::Proxies => AgentRequest::GetProxyManager,
    Command::AddProxy {
      id,
      domain,
      port,
      target,
      allowed_origins,
    } => AgentRequest::AddProxyConnection(ProxyConnectionInput {
      id,
      domain,
      listen_port: port,
      target,
      allowed_origins,
    }),
    Command::UpdateProxy {
      id,
      domain,
      port,
      target,
      allowed_origins,
    } => AgentRequest::UpdateProxyConnection {
      connection_id: id.clone(),
      input: ProxyConnectionInput {
        id,
        domain,
        listen_port: port,
        target,
        allowed_origins,
      },
    },
    Command::RemoveProxy { id } => AgentRequest::RemoveProxyConnection { connection_id: id },
    Command::StartProxy { id } => AgentRequest::StartProxyConnection { connection_id: id },
    Command::StopProxy { id } => AgentRequest::StopProxyConnection { connection_id: id },
    Command::StartAllProxies => AgentRequest::StartAllProxyConnections,
    Command::StopAllProxies => AgentRequest::StopAllProxyConnections,
    Command::Start => AgentRequest::StartAll,
    Command::Stop => AgentRequest::StopAll,
    Command::StartMariaDb => AgentRequest::StartMariaDb,
    Command::StopMariaDb => AgentRequest::StopMariaDb,
    Command::InstallMariaDbRuntime { artifact, release } => AgentRequest::InstallMariaDbRuntime {
      artifact_path: artifact,
      release_path: release,
    },
    Command::RemoveMariaDbRuntime => AgentRequest::RemoveMariaDbRuntime,
    Command::InstallRuntime { .. } => unreachable!("handled before Agent connection"),
    Command::SeedDemo { .. } => unreachable!("handled before Agent connection"),
  };
  let response = send_request(&endpoint, request).await?;
  println!("{}", serde_json::to_string_pretty(&response)?);
  if matches!(response, AgentResponse::Error { .. }) {
    std::process::exit(1);
  }
  Ok(())
}

#[cfg(target_os = "macos")]
fn trust_local_ca(certificate_path: &std::path::Path) -> Result<()> {
  let home = std::env::var_os("HOME").context("HOME is not defined")?;
  let login_keychain = PathBuf::from(home).join("Library/Keychains/login.keychain-db");
  if !login_keychain.is_file() {
    anyhow::bail!(
      "macOS Login keychain is missing: {}",
      login_keychain.display()
    );
  }
  let already_trusted = std::process::Command::new("/usr/bin/security")
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
  let output = std::process::Command::new("/usr/bin/security")
    .arg("add-trusted-cert")
    .arg("-r")
    .arg("trustRoot")
    .arg("-k")
    .arg(&login_keychain)
    .arg(certificate_path)
    .output()
    .context("unable to update the macOS Login keychain")?;
  if !output.status.success() {
    anyhow::bail!(
      "unable to trust the fabDev local CA: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(())
}

#[cfg(windows)]
fn trust_local_ca(certificate_path: &std::path::Path) -> Result<()> {
  let executable = std::env::current_exe().context("unable to locate fabDev CLI")?;
  let helper = executable
    .parent()
    .context("fabDev CLI executable has no parent directory")?
    .join("fabdev-windows-helper.exe");
  let status = std::process::Command::new(&helper)
    .arg("trust-ca")
    .arg("--certificate")
    .arg(certificate_path)
    .status()
    .with_context(|| format!("unable to start Windows Helper at {}", helper.display()))?;
  if !status.success() {
    anyhow::bail!("fabDev Windows Helper could not trust the local CA");
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn trust_local_ca(_certificate_path: &std::path::Path) -> Result<()> {
  anyhow::bail!("local CA trust is not supported on this platform")
}

fn seed_demo(paths: &AppPaths, project_path: &std::path::Path) -> Result<bool> {
  paths.ensure()?;
  let repository = SiteRepository::open(paths.database())?;
  if !repository.list()?.is_empty() {
    return Ok(false);
  }
  let site = create_site(SiteInput {
    name: Some("fabDev Demo".to_owned()),
    domain: Some("demo.test".to_owned()),
    project_path: project_path.to_path_buf(),
    document_root: Some(PathBuf::from("public")),
    php_version: Some("8.2".parse()?),
  })?;
  repository.insert(&site)?;
  Ok(true)
}

async fn send_request(endpoint: &AgentEndpoint, request: AgentRequest) -> Result<AgentResponse> {
  #[cfg(unix)]
  let stream = {
    let AgentEndpoint::UnixSocket(socket) = endpoint;
    UnixStream::connect(socket)
      .await
      .with_context(|| format!("unable to connect to fabDev Agent at {}", socket.display()))?
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
  let line = lines
    .next_line()
    .await?
    .context("fabDev Agent closed the connection")?;
  serde_json::from_str(&line).context("fabDev Agent returned an invalid response")
}

#[cfg(windows)]
async fn connect_named_pipe(pipe_name: &str) -> Result<NamedPipeClient> {
  let started = tokio::time::Instant::now();
  loop {
    match ClientOptions::new().open(pipe_name) {
      Ok(client) => return Ok(client),
      Err(error) if error.raw_os_error() == Some(231) && started.elapsed().as_secs() < 5 => {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
      }
      Err(error) => {
        return Err(error)
          .with_context(|| format!("unable to connect to fabDev Agent at {pipe_name}"));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn seeds_only_one_demo_site_into_an_empty_registry() {
    let root = std::env::temp_dir().join(format!("fabdev-community-{}", uuid::Uuid::new_v4()));
    let project = root.join("demo");
    std::fs::create_dir_all(project.join("public")).expect("create demo fixture");
    std::fs::write(project.join("public/index.php"), "<?php echo 'fabDev';")
      .expect("write demo fixture");
    let paths = AppPaths::from_root(root.join("data"));

    assert!(seed_demo(&paths, &project).expect("seed first demo"));
    assert!(!seed_demo(&paths, &project).expect("skip second demo"));
    let sites = SiteRepository::open(paths.database())
      .expect("open seeded registry")
      .list()
      .expect("list seeded Sites");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].domain, "demo.test");
    assert_eq!(
      sites[0]
        .php_version
        .as_ref()
        .expect("demo PHP version")
        .to_string(),
      "8.2"
    );

    std::fs::remove_dir_all(root).expect("remove demo fixture");
  }
}
