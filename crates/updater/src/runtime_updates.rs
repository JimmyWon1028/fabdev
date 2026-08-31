use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use fabdev_runtime::{
  parse_and_validate_runtime_catalog, AcceptedRuntimeCatalog, RuntimeCatalogValidation,
  RuntimeRelease, ValidatedRuntimeCatalog, RUNTIME_CATALOG_MAX_BYTES, RUNTIME_CATALOG_URL,
};

const RUNTIME_UPDATE_DIRECTORY: &str = "runtime-updates";
const RUNTIME_UPDATE_PENDING_DIRECTORY: &str = "pending";
const RUNTIME_CATALOG_FILE: &str = "fabdev-runtime-v1.json";
const ACCEPTED_CATALOG_FILE: &str = "accepted-catalog.json";
const MAX_REDIRECTS: usize = 10;
#[cfg(debug_assertions)]
const RUNTIME_TEST_BASE_URL_ENV: &str = "FABDEV_RUNTIME_TEST_BASE_URL";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadedRuntimeUpdate {
  pub name: String,
  pub version: String,
  pub platform: String,
  pub architecture: String,
  pub file_name: String,
  pub size: u64,
  pub sha256: String,
  pub path: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeDownloadRequest<'a> {
  pub cache_directory: &'a Path,
  pub current_app_version: &'a str,
  pub current_agent_protocol_version: u16,
  pub name: &'a str,
  pub version: &'a str,
  pub platform: &'a str,
  pub architecture: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptedCatalogFile {
  sequence: u64,
  sha256: String,
}

pub async fn check_for_runtime_updates(
  cache_directory: &Path,
  current_app_version: &str,
  current_agent_protocol_version: u16,
) -> anyhow::Result<ValidatedRuntimeCatalog> {
  let test_base_url = runtime_test_base_url()?;
  let client = runtime_http_client(test_base_url.as_ref())?;
  let catalog_url = runtime_transport_url(
    test_base_url.as_ref(),
    RUNTIME_CATALOG_URL,
    RUNTIME_CATALOG_FILE,
  )?;
  let contents = fetch_runtime_catalog(&client, catalog_url.as_str()).await?;
  let accepted = load_accepted_catalog(cache_directory).await?;
  let validation = runtime_catalog_validation(
    current_app_version,
    current_agent_protocol_version,
    accepted.as_ref(),
  )?;
  let validated = parse_and_validate_runtime_catalog(&contents, &validation)
    .context("the Runtime Catalog failed validation")?;
  persist_runtime_catalog(cache_directory, &contents, &validated).await?;
  Ok(validated)
}

pub async fn cached_runtime_catalog(
  cache_directory: &Path,
  current_app_version: &str,
  current_agent_protocol_version: u16,
) -> anyhow::Result<ValidatedRuntimeCatalog> {
  let root = runtime_update_root(cache_directory);
  let contents = tokio::fs::read(root.join(RUNTIME_CATALOG_FILE))
    .await
    .context("no verified Runtime Catalog is cached; check for Runtime updates first")?;
  let accepted = load_accepted_catalog(cache_directory)
    .await?
    .context("accepted Runtime Catalog state is missing; check for Runtime updates again")?;
  let validation = runtime_catalog_validation(
    current_app_version,
    current_agent_protocol_version,
    Some(&accepted),
  )?;
  parse_and_validate_runtime_catalog(&contents, &validation)
    .context("the cached Runtime Catalog failed validation")
}

pub async fn download_cached_runtime_update<F, C>(
  request: RuntimeDownloadRequest<'_>,
  mut on_progress: F,
  is_cancelled: C,
) -> anyhow::Result<DownloadedRuntimeUpdate>
where
  F: FnMut(u64, u64),
  C: Fn() -> bool,
{
  let catalog = cached_runtime_catalog(
    request.cache_directory,
    request.current_app_version,
    request.current_agent_protocol_version,
  )
  .await?;
  let release = select_runtime_release(
    &catalog,
    request.name,
    request.version,
    request.platform,
    request.architecture,
  )?
  .clone();
  if is_cancelled() {
    bail!("Runtime download was cancelled");
  }

  let pending_directory =
    runtime_update_root(request.cache_directory).join(RUNTIME_UPDATE_PENDING_DIRECTORY);
  tokio::fs::create_dir_all(&pending_directory)
    .await
    .context("unable to create the Runtime update download directory")?;
  let file_name = release
    .file_name
    .as_deref()
    .context("the cached Runtime entry is missing its file name")?;
  let target = pending_directory.join(file_name);
  if target.is_file() && verify_runtime_artifact(&target, &release).await.is_ok() {
    on_progress(release.size, release.size);
    return Ok(downloaded_runtime_update(&release, target));
  }

  remove_file_if_exists(&target).await?;
  let partial = pending_directory.join(format!("{file_name}.part"));
  on_progress(0, release.size);

  let test_base_url = runtime_test_base_url()?;
  let client = runtime_http_client(test_base_url.as_ref())?;
  let transport_url = runtime_transport_url(
    test_base_url.as_ref(),
    &release.url,
    release
      .file_name
      .as_deref()
      .context("the cached Runtime entry is missing its file name")?,
  )?;
  let windows_x64 = request.platform == "windows" && request.architecture == "x64";
  let result = if windows_x64 {
    crate::windows_download::download_windows_artifact(
      crate::windows_download::WindowsArtifactDownload {
        client: &client,
        url: transport_url.as_str(),
        size: release.size,
        sha256: &release.sha256,
        partial: &partial,
        target: &target,
      },
      &mut on_progress,
      &is_cancelled,
    )
    .await
  } else {
    let mut transport_release = release.clone();
    transport_release.url = transport_url.to_string();
    download_runtime_artifact(
      &client,
      &transport_release,
      &partial,
      &target,
      &mut on_progress,
      &is_cancelled,
    )
    .await
  };
  result?;
  Ok(downloaded_runtime_update(&release, target))
}

pub async fn verified_cached_runtime_update(
  request: RuntimeDownloadRequest<'_>,
) -> anyhow::Result<DownloadedRuntimeUpdate> {
  let catalog = cached_runtime_catalog(
    request.cache_directory,
    request.current_app_version,
    request.current_agent_protocol_version,
  )
  .await?;
  let release = select_runtime_release(
    &catalog,
    request.name,
    request.version,
    request.platform,
    request.architecture,
  )?;
  let file_name = release
    .file_name
    .as_deref()
    .context("the cached Runtime entry is missing its file name")?;
  let path = runtime_update_root(request.cache_directory)
    .join(RUNTIME_UPDATE_PENDING_DIRECTORY)
    .join(file_name);
  verify_runtime_artifact(&path, release).await?;
  Ok(downloaded_runtime_update(release, path))
}

pub async fn cleanup_runtime_update_partials(cache_directory: &Path) -> anyhow::Result<usize> {
  let root = runtime_update_root(cache_directory);
  let mut removed = remove_partials_in_directory(&root).await?;
  removed += remove_partials_in_directory(&root.join(RUNTIME_UPDATE_PENDING_DIRECTORY)).await?;
  Ok(removed)
}

fn runtime_http_client(test_base_url: Option<&Url>) -> anyhow::Result<Client> {
  let test_origin = test_base_url.map(|url| {
    (
      url.scheme().to_owned(),
      url
        .host_str()
        .expect("test URL host was validated")
        .to_owned(),
      url.port(),
    )
  });
  Client::builder()
    .user_agent(format!("fabDev/{}", env!("CARGO_PKG_VERSION")))
    .connect_timeout(Duration::from_secs(15))
    .timeout(Duration::from_secs(30 * 60))
    .https_only(test_base_url.is_none())
    .redirect(Policy::custom(move |attempt| {
      if attempt.previous().len() >= MAX_REDIRECTS {
        return attempt.error("Runtime update exceeded the redirect limit");
      }
      let url = attempt.url();
      let allowed = match &test_origin {
        Some((scheme, host, port)) => {
          url.scheme() == scheme && url.host_str() == Some(host.as_str()) && url.port() == *port
        }
        None => url.scheme() == "https" && is_allowed_runtime_update_host(url.host_str()),
      };
      if !allowed {
        return attempt.error("Runtime update redirect left the HTTPS GitHub asset allowlist");
      }
      attempt.follow()
    }))
    .build()
    .context("unable to initialize the Runtime update HTTPS client")
}

fn runtime_test_base_url() -> anyhow::Result<Option<Url>> {
  #[cfg(debug_assertions)]
  {
    let Some(value) = std::env::var_os(RUNTIME_TEST_BASE_URL_ENV) else {
      return Ok(None);
    };
    let value = value
      .into_string()
      .map_err(|_| anyhow::anyhow!("{RUNTIME_TEST_BASE_URL_ENV} must be valid UTF-8"))?;
    parse_runtime_test_base_url(&value).map(Some)
  }

  #[cfg(not(debug_assertions))]
  Ok(None)
}

#[cfg(debug_assertions)]
fn parse_runtime_test_base_url(value: &str) -> anyhow::Result<Url> {
  let url = Url::parse(value).context("unable to parse the Runtime test base URL")?;
  let valid = url.scheme() == "http"
    && url.host_str() == Some("127.0.0.1")
    && url.port().is_some()
    && url.username().is_empty()
    && url.password().is_none()
    && url.query().is_none()
    && url.fragment().is_none()
    && url.path() == "/";
  if !valid {
    bail!(
      "{RUNTIME_TEST_BASE_URL_ENV} must be an http://127.0.0.1:<port> origin without credentials, path, query, or fragment"
    );
  }
  Ok(url)
}

fn runtime_transport_url(
  test_base_url: Option<&Url>,
  production_url: &str,
  file_name: &str,
) -> anyhow::Result<Url> {
  match test_base_url {
    Some(base_url) => base_url
      .join(file_name)
      .context("unable to construct the Runtime test transport URL"),
    None => Url::parse(production_url).context("unable to parse the Runtime transport URL"),
  }
}

fn is_allowed_runtime_update_host(host: Option<&str>) -> bool {
  matches!(
    host,
    Some(
      "github.com"
        | "release-assets.githubusercontent.com"
        | "objects.githubusercontent.com"
        | "github-releases.githubusercontent.com"
    )
  )
}

async fn fetch_runtime_catalog(client: &Client, url: &str) -> anyhow::Result<Vec<u8>> {
  let response = client
    .get(url)
    .timeout(Duration::from_secs(30))
    .send()
    .await
    .context("unable to download the Runtime Catalog")?
    .error_for_status()
    .context("the Runtime Catalog returned an unsuccessful status")?;
  if response
    .content_length()
    .is_some_and(|length| length > RUNTIME_CATALOG_MAX_BYTES as u64)
  {
    bail!("Runtime Catalog exceeds the size limit");
  }
  let mut contents = Vec::new();
  let mut stream = response.bytes_stream();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.context("unable to read the Runtime Catalog download")?;
    if contents.len() + chunk.len() > RUNTIME_CATALOG_MAX_BYTES {
      bail!("Runtime Catalog exceeds the size limit");
    }
    contents.extend_from_slice(&chunk);
  }
  Ok(contents)
}

async fn download_runtime_artifact<F, C>(
  client: &Client,
  release: &RuntimeRelease,
  partial: &Path,
  target: &Path,
  on_progress: &mut F,
  is_cancelled: &C,
) -> anyhow::Result<()>
where
  F: FnMut(u64, u64),
  C: Fn() -> bool,
{
  if is_cancelled() {
    bail!("Runtime download was cancelled");
  }
  let (mut downloaded, mut hasher) = resumable_runtime_partial(partial, release).await?;
  if downloaded == release.size {
    if let Err(error) = verify_runtime_artifact(partial, release).await {
      remove_file_if_exists(partial).await?;
      return Err(error).context("the completed Runtime partial failed verification");
    }
    tokio::fs::rename(partial, target)
      .await
      .context("unable to finalize the verified Runtime package")?;
    on_progress(downloaded, release.size);
    return Ok(());
  }

  on_progress(downloaded, release.size);
  let mut request = client.get(&release.url);
  if downloaded > 0 {
    request = request.header(RANGE, format!("bytes={downloaded}-"));
  }
  let response = request
    .send()
    .await
    .context("unable to download the Runtime package")?;
  let append = if downloaded == 0 {
    response
      .error_for_status_ref()
      .context("the Runtime package returned an unsuccessful status")?;
    if response.status() != StatusCode::OK {
      bail!(
        "Runtime package returned an invalid initial response: HTTP {}",
        response.status()
      );
    }
    false
  } else if response.status() == StatusCode::PARTIAL_CONTENT {
    validate_runtime_content_range(&response, downloaded, release.size)?;
    true
  } else if response.status() == StatusCode::OK {
    downloaded = 0;
    hasher = Sha256::new();
    false
  } else {
    response
      .error_for_status_ref()
      .context("the Runtime package returned an unsuccessful status")?;
    bail!(
      "Runtime package returned an invalid resume response: HTTP {}",
      response.status()
    );
  };

  let expected_length = release
    .size
    .checked_sub(downloaded)
    .context("Runtime package resume offset exceeds the Catalog size")?;
  if response
    .content_length()
    .is_some_and(|length| length != expected_length)
  {
    if append {
      remove_file_if_exists(partial).await?;
    }
    bail!("Runtime package size does not match the Catalog");
  }

  let mut file = if append {
    tokio::fs::OpenOptions::new()
      .append(true)
      .open(partial)
      .await
      .context("unable to reopen the partial Runtime package")?
  } else {
    tokio::fs::File::create(partial)
      .await
      .context("unable to create the partial Runtime package")?
  };
  let mut stream = response.bytes_stream();
  while let Some(result) = stream.next().await {
    if is_cancelled() {
      drop(file);
      remove_file_if_exists(partial).await?;
      bail!("Runtime download was cancelled");
    }
    let chunk = match result {
      Ok(chunk) => chunk,
      Err(error) => {
        file
          .flush()
          .await
          .context("unable to flush the interrupted Runtime package")?;
        file
          .sync_all()
          .await
          .context("unable to sync the interrupted Runtime package")?;
        return Err(error).context("unable to read the Runtime package download");
      }
    };
    downloaded = downloaded
      .checked_add(chunk.len() as u64)
      .context("Runtime package size overflow")?;
    if downloaded > release.size {
      drop(file);
      remove_file_if_exists(partial).await?;
      bail!("Runtime package exceeds the Catalog size");
    }
    file
      .write_all(&chunk)
      .await
      .context("unable to write the Runtime package")?;
    hasher.update(&chunk);
    on_progress(downloaded, release.size);
  }
  file
    .flush()
    .await
    .context("unable to flush the Runtime package")?;
  file
    .sync_all()
    .await
    .context("unable to sync the Runtime package")?;
  drop(file);

  if is_cancelled() {
    remove_file_if_exists(partial).await?;
    bail!("Runtime download was cancelled");
  }
  if downloaded != release.size {
    bail!("Runtime package download is incomplete");
  }
  if hex::encode(hasher.finalize()) != release.sha256 {
    remove_file_if_exists(partial).await?;
    bail!("Runtime package SHA-256 does not match the Catalog");
  }
  tokio::fs::rename(partial, target)
    .await
    .context("unable to finalize the verified Runtime package")
}

async fn verify_runtime_artifact(path: &Path, release: &RuntimeRelease) -> anyhow::Result<()> {
  let metadata = tokio::fs::symlink_metadata(path)
    .await
    .context("verified Runtime package is missing")?;
  if !metadata.file_type().is_file() || metadata.len() != release.size {
    bail!("downloaded Runtime package size does not match the Catalog");
  }
  let mut file = tokio::fs::File::open(path)
    .await
    .context("unable to open the downloaded Runtime package")?;
  let mut buffer = vec![0_u8; 64 * 1024];
  let mut hasher = Sha256::new();
  loop {
    let count = file
      .read(&mut buffer)
      .await
      .context("unable to verify the downloaded Runtime package")?;
    if count == 0 {
      break;
    }
    hasher.update(&buffer[..count]);
  }
  if hex::encode(hasher.finalize()) != release.sha256 {
    bail!("downloaded Runtime package SHA-256 does not match the Catalog");
  }
  Ok(())
}

async fn resumable_runtime_partial(
  partial: &Path,
  release: &RuntimeRelease,
) -> anyhow::Result<(u64, Sha256)> {
  let metadata = match tokio::fs::symlink_metadata(partial).await {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, Sha256::new())),
    Err(error) => return Err(error).context("unable to inspect the partial Runtime package"),
  };
  if !metadata.file_type().is_file() || metadata.len() > release.size {
    remove_file_if_exists(partial).await?;
    return Ok((0, Sha256::new()));
  }

  let mut file = tokio::fs::File::open(partial)
    .await
    .context("unable to open the partial Runtime package")?;
  let mut buffer = vec![0_u8; 64 * 1024];
  let mut hasher = Sha256::new();
  loop {
    let count = file
      .read(&mut buffer)
      .await
      .context("unable to read the partial Runtime package")?;
    if count == 0 {
      break;
    }
    hasher.update(&buffer[..count]);
  }
  Ok((metadata.len(), hasher))
}

fn validate_runtime_content_range(
  response: &reqwest::Response,
  start: u64,
  total: u64,
) -> anyhow::Result<()> {
  let end = total
    .checked_sub(1)
    .context("Runtime package has an invalid zero size")?;
  let expected = format!("bytes {start}-{end}/{total}");
  if response
    .headers()
    .get(CONTENT_RANGE)
    .and_then(|value| value.to_str().ok())
    != Some(expected.as_str())
  {
    bail!("Runtime package returned an invalid Content-Range");
  }
  Ok(())
}

async fn persist_runtime_catalog(
  cache_directory: &Path,
  contents: &[u8],
  validated: &ValidatedRuntimeCatalog,
) -> anyhow::Result<()> {
  let root = runtime_update_root(cache_directory);
  tokio::fs::create_dir_all(&root)
    .await
    .context("unable to create the Runtime update cache directory")?;
  write_file_atomically(&root.join(RUNTIME_CATALOG_FILE), contents).await?;
  let mut state = serde_json::to_vec_pretty(&AcceptedCatalogFile {
    sequence: validated.catalog.catalog_sequence,
    sha256: validated.sha256.clone(),
  })
  .context("unable to serialize the accepted Runtime Catalog state")?;
  state.push(b'\n');
  write_file_atomically(&root.join(ACCEPTED_CATALOG_FILE), &state).await
}

async fn load_accepted_catalog(
  cache_directory: &Path,
) -> anyhow::Result<Option<AcceptedRuntimeCatalog>> {
  let path = runtime_update_root(cache_directory).join(ACCEPTED_CATALOG_FILE);
  let contents = match tokio::fs::read(&path).await {
    Ok(contents) => contents,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error).context("unable to read the accepted Runtime Catalog state"),
  };
  if contents.len() > 1024 {
    bail!("accepted Runtime Catalog state exceeds the size limit");
  }
  let state = serde_json::from_slice::<AcceptedCatalogFile>(&contents)
    .context("accepted Runtime Catalog state is invalid")?;
  Ok(Some(AcceptedRuntimeCatalog {
    sequence: state.sequence,
    sha256: state.sha256,
  }))
}

async fn write_file_atomically(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
  let file_name = path
    .file_name()
    .and_then(|name| name.to_str())
    .context("Runtime update cache path has no valid file name")?;
  let partial = path.with_file_name(format!("{file_name}.part"));
  remove_file_if_exists(&partial).await?;
  let mut file = tokio::fs::File::create(&partial).await.with_context(|| {
    format!(
      "unable to create Runtime update cache file: {}",
      partial.display()
    )
  })?;
  file
    .write_all(contents)
    .await
    .context("unable to write Runtime update cache file")?;
  file
    .sync_all()
    .await
    .context("unable to sync Runtime update cache file")?;
  drop(file);
  remove_file_if_exists(path).await?;
  tokio::fs::rename(&partial, path)
    .await
    .context("unable to finalize Runtime update cache file")
}

async fn remove_partials_in_directory(directory: &Path) -> anyhow::Result<usize> {
  let mut entries = match tokio::fs::read_dir(directory).await {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
    Err(error) => return Err(error.into()),
  };
  let mut removed = 0;
  while let Some(entry) = entries.next_entry().await? {
    let path = entry.path();
    let is_partial = path
      .file_name()
      .and_then(|name| name.to_str())
      .is_some_and(|name| name.ends_with(".part"));
    if is_partial && entry.file_type().await?.is_file() {
      tokio::fs::remove_file(path).await?;
      removed += 1;
    }
  }
  Ok(removed)
}

async fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
  match tokio::fs::remove_file(path).await {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn runtime_catalog_validation<'a>(
  current_app_version: &'a str,
  current_agent_protocol_version: u16,
  accepted_catalog: Option<&'a AcceptedRuntimeCatalog>,
) -> anyhow::Result<RuntimeCatalogValidation<'a>> {
  let now_unix_seconds = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .context("system time is before the Unix epoch")?
    .as_secs()
    .try_into()
    .context("system time exceeds the Runtime Catalog timestamp range")?;
  Ok(RuntimeCatalogValidation {
    current_app_version,
    current_agent_protocol_version,
    now_unix_seconds,
    accepted_catalog,
  })
}

fn select_runtime_release<'a>(
  catalog: &'a ValidatedRuntimeCatalog,
  name: &str,
  version: &str,
  platform: &str,
  architecture: &str,
) -> anyhow::Result<&'a RuntimeRelease> {
  catalog
    .catalog
    .runtimes
    .iter()
    .find(|release| {
      release.name == name
        && release.version == version
        && release.platform == platform
        && release.architecture == architecture
    })
    .context("the cached Runtime Catalog does not contain the requested Runtime")
}

fn downloaded_runtime_update(release: &RuntimeRelease, path: PathBuf) -> DownloadedRuntimeUpdate {
  DownloadedRuntimeUpdate {
    name: release.name.clone(),
    version: release.version.clone(),
    platform: release.platform.clone(),
    architecture: release.architecture.clone(),
    file_name: release
      .file_name
      .clone()
      .expect("validated Runtime entry has a file name"),
    size: release.size,
    sha256: release.sha256.clone(),
    path,
  }
}

fn runtime_update_root(cache_directory: &Path) -> PathBuf {
  cache_directory.join(RUNTIME_UPDATE_DIRECTORY)
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::sync::Arc;

  use super::*;
  use fabdev_runtime::{
    generate_runtime_catalog, RuntimeCatalog, RuntimeCatalogCompatibility,
    RuntimeSourceVerification, RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION, RUNTIME_CATALOG_PRODUCT,
    RUNTIME_CATALOG_SCHEMA_VERSION,
  };
  use uuid::Uuid;

  fn catalog() -> RuntimeCatalog {
    RuntimeCatalog {
      schema_version: RUNTIME_CATALOG_SCHEMA_VERSION,
      product: RUNTIME_CATALOG_PRODUCT.to_owned(),
      channel: "community".to_owned(),
      catalog_sequence: 2,
      generated_at: "2026-08-30T00:00:00Z".to_owned(),
      expires_at: "2027-02-26T00:00:00Z".to_owned(),
      unsigned_community_build: true,
      integrity: "sha256".to_owned(),
      compatibility: RuntimeCatalogCompatibility {
        minimum_app_version: "0.1.4".to_owned(),
        minimum_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
      },
      signature: None,
      runtimes: vec![RuntimeRelease {
        name: "php".to_owned(),
        version: "8.4.24".to_owned(),
        platform: "macos".to_owned(),
        architecture: "arm64".to_owned(),
        minimum_os_version: Some("13.0".to_owned()),
        file_name: Some("php-8.4.24-macos-arm64-community.tar.gz".to_owned()),
        url: "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-8.4.24-macos-arm64-community.tar.gz".to_owned(),
        size: 7,
        sha256: "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5".to_owned(),
        signature: None,
        source_verification: Some(RuntimeSourceVerification {
          method: "pgp".to_owned(),
          fingerprint: Some("9D7F99A0CB8F05C8A6958D6256A97AF7600A39A6".to_owned()),
          upstream_sha256: "a".repeat(64),
        }),
        archive_format: Some("tar.gz".to_owned()),
        install_mode: Some("side-by-side".to_owned()),
        health_check_profile: Some("php-runtime-v1".to_owned()),
      }],
    }
  }

  fn validation() -> RuntimeCatalogValidation<'static> {
    RuntimeCatalogValidation {
      current_app_version: "0.1.4",
      current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
      now_unix_seconds: 1_788_048_060,
      accepted_catalog: None,
    }
  }

  #[test]
  fn restricts_the_debug_runtime_feed_to_an_explicit_loopback_origin() {
    assert_eq!(
      parse_runtime_test_base_url("http://127.0.0.1:48123/")
        .expect("accept loopback origin")
        .as_str(),
      "http://127.0.0.1:48123/"
    );
    for invalid in [
      "https://127.0.0.1:48123/",
      "http://localhost:48123/",
      "http://127.0.0.1/",
      "http://127.0.0.1:48123/feed/",
      "http://user@127.0.0.1:48123/",
      "http://127.0.0.1:48123/?source=test",
    ] {
      assert!(
        parse_runtime_test_base_url(invalid).is_err(),
        "reject {invalid}"
      );
    }
  }

  #[test]
  fn rewrites_only_the_debug_runtime_transport_location() {
    let base =
      parse_runtime_test_base_url("http://127.0.0.1:48123/").expect("parse loopback origin");
    assert_eq!(
      runtime_transport_url(
        Some(&base),
        RUNTIME_CATALOG_URL,
        "php-8.4.24-macos-arm64-community.tar.gz"
      )
      .expect("rewrite transport URL")
      .as_str(),
      "http://127.0.0.1:48123/php-8.4.24-macos-arm64-community.tar.gz"
    );
    assert_eq!(
      runtime_transport_url(None, RUNTIME_CATALOG_URL, RUNTIME_CATALOG_FILE)
        .expect("keep production URL")
        .as_str(),
      RUNTIME_CATALOG_URL
    );
  }

  #[tokio::test]
  async fn persists_and_reloads_the_validated_runtime_catalog() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-cache-{}", Uuid::new_v4()));
    let contents = generate_runtime_catalog(&catalog(), &validation()).expect("generate Catalog");
    let validated =
      parse_and_validate_runtime_catalog(&contents, &validation()).expect("validate Catalog");
    persist_runtime_catalog(&root, &contents, &validated)
      .await
      .expect("persist Catalog");

    let cached = cached_runtime_catalog(&root, "0.1.4", RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION)
      .await
      .expect("reload Catalog");
    assert_eq!(cached.catalog.catalog_sequence, 2);
    assert_eq!(cached.sha256, validated.sha256);
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn cancels_before_network_and_removes_only_partial_files() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-cancel-{}", Uuid::new_v4()));
    let contents = generate_runtime_catalog(&catalog(), &validation()).expect("generate Catalog");
    let validated =
      parse_and_validate_runtime_catalog(&contents, &validation()).expect("validate Catalog");
    persist_runtime_catalog(&root, &contents, &validated)
      .await
      .expect("persist Catalog");
    let pending = runtime_update_root(&root).join(RUNTIME_UPDATE_PENDING_DIRECTORY);
    tokio::fs::create_dir_all(&pending)
      .await
      .expect("create pending");
    tokio::fs::write(pending.join("stale.tar.gz.part"), b"partial")
      .await
      .expect("write partial");
    tokio::fs::write(pending.join("verified.tar.gz"), b"verified")
      .await
      .expect("write verified");

    let error = download_cached_runtime_update(
      RuntimeDownloadRequest {
        cache_directory: &root,
        current_app_version: "0.1.4",
        current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
        name: "php",
        version: "8.4.24",
        platform: "macos",
        architecture: "arm64",
      },
      |_, _| {},
      || true,
    )
    .await
    .expect_err("cancel before network");
    assert!(error.to_string().contains("cancelled"));
    assert_eq!(
      cleanup_runtime_update_partials(&root)
        .await
        .expect("cleanup"),
      1
    );
    assert!(pending.join("verified.tar.gz").is_file());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn reuses_only_a_verified_cached_runtime_package() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-retry-{}", Uuid::new_v4()));
    let contents = generate_runtime_catalog(&catalog(), &validation()).expect("generate Catalog");
    let validated =
      parse_and_validate_runtime_catalog(&contents, &validation()).expect("validate Catalog");
    persist_runtime_catalog(&root, &contents, &validated)
      .await
      .expect("persist Catalog");
    let pending = runtime_update_root(&root).join(RUNTIME_UPDATE_PENDING_DIRECTORY);
    tokio::fs::create_dir_all(&pending)
      .await
      .expect("create pending");
    let file_name = "php-8.4.24-macos-arm64-community.tar.gz";
    tokio::fs::write(pending.join(file_name), b"payload")
      .await
      .expect("write verified package");
    let progress = Arc::new(AtomicU64::new(0));
    let reported = Arc::clone(&progress);

    let downloaded = download_cached_runtime_update(
      RuntimeDownloadRequest {
        cache_directory: &root,
        current_app_version: "0.1.4",
        current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
        name: "php",
        version: "8.4.24",
        platform: "macos",
        architecture: "arm64",
      },
      move |downloaded, _| reported.store(downloaded, Ordering::Relaxed),
      || false,
    )
    .await
    .expect("reuse verified package");

    assert_eq!(downloaded.path, pending.join(file_name));
    assert_eq!(progress.load(Ordering::Relaxed), 7);
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn revalidates_catalog_identity_and_package_before_install() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-install-{}", Uuid::new_v4()));
    let contents = generate_runtime_catalog(&catalog(), &validation()).expect("generate Catalog");
    let validated =
      parse_and_validate_runtime_catalog(&contents, &validation()).expect("validate Catalog");
    persist_runtime_catalog(&root, &contents, &validated)
      .await
      .expect("persist Catalog");
    let pending = runtime_update_root(&root).join(RUNTIME_UPDATE_PENDING_DIRECTORY);
    tokio::fs::create_dir_all(&pending)
      .await
      .expect("create pending");
    let file_name = "php-8.4.24-macos-arm64-community.tar.gz";
    tokio::fs::write(pending.join(file_name), b"payload")
      .await
      .expect("write verified package");
    let request = RuntimeDownloadRequest {
      cache_directory: &root,
      current_app_version: "0.1.4",
      current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
      name: "php",
      version: "8.4.24",
      platform: "macos",
      architecture: "arm64",
    };

    let verified = verified_cached_runtime_update(request)
      .await
      .expect("revalidate package");
    assert_eq!(verified.path, pending.join(file_name));

    tokio::fs::write(&verified.path, b"tampered")
      .await
      .expect("tamper package");
    let error = verified_cached_runtime_update(request)
      .await
      .expect_err("reject changed package");
    assert!(error.to_string().contains("size does not match"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn rejects_a_cached_catalog_without_accepted_state() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-state-{}", Uuid::new_v4()));
    let update_root = runtime_update_root(&root);
    tokio::fs::create_dir_all(&update_root)
      .await
      .expect("create Runtime update cache");
    let contents = generate_runtime_catalog(&catalog(), &validation()).expect("generate Catalog");
    tokio::fs::write(update_root.join(RUNTIME_CATALOG_FILE), contents)
      .await
      .expect("write unaccepted Catalog");

    let error = cached_runtime_catalog(&root, "0.1.4", RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION)
      .await
      .expect_err("reject unaccepted cached Catalog");
    assert!(error
      .to_string()
      .contains("accepted Runtime Catalog state is missing"));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn streams_verifies_and_atomically_finishes_a_runtime_package() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind fixture server");
    let address = listener.local_addr().expect("read fixture address");
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.expect("accept fixture request");
      let mut request = [0_u8; 1024];
      let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut request)
        .await
        .expect("read fixture request");
      tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\npayload",
      )
      .await
      .expect("write fixture response");
    });

    let root = std::env::temp_dir().join(format!("fabdev-runtime-stream-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create fixture");
    let partial = root.join("runtime.tar.gz.part");
    let target = root.join("runtime.tar.gz");
    let mut release = catalog().runtimes.remove(0);
    release.url = format!("http://{address}/runtime.tar.gz");
    let progress = Arc::new(AtomicU64::new(0));
    let reported = Arc::clone(&progress);
    let client = Client::builder().build().expect("build fixture client");

    download_runtime_artifact(
      &client,
      &release,
      &partial,
      &target,
      &mut |downloaded, _| reported.store(downloaded, Ordering::Relaxed),
      &|| false,
    )
    .await
    .expect("download Runtime fixture");
    server.await.expect("join fixture server");

    assert_eq!(progress.load(Ordering::Relaxed), 7);
    assert_eq!(
      tokio::fs::read(&target).await.expect("read target"),
      b"payload"
    );
    assert!(!partial.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn resumes_an_interrupted_runtime_package_with_a_valid_range() {
    let payload = b"payload";
    let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind interrupted fixture server");
    let first_address = first_listener.local_addr().expect("read fixture address");
    let first_server = tokio::spawn(async move {
      let (mut stream, _) = first_listener
        .accept()
        .await
        .expect("accept interrupted fixture request");
      let mut request = [0_u8; 1024];
      let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut request)
        .await
        .expect("read interrupted fixture request");
      tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\npay",
      )
      .await
      .expect("write interrupted fixture response");
    });

    let root = std::env::temp_dir().join(format!("fabdev-runtime-resume-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create resume fixture");
    let partial = root.join("runtime.tar.gz.part");
    let target = root.join("runtime.tar.gz");
    let mut release = catalog().runtimes.remove(0);
    release.url = format!("http://{first_address}/runtime.tar.gz");
    let client = Client::builder().build().expect("build fixture client");

    let error = download_runtime_artifact(
      &client,
      &release,
      &partial,
      &target,
      &mut |_, _| {},
      &|| false,
    )
    .await
    .expect_err("interrupt Runtime fixture");
    first_server.await.expect("join interrupted fixture server");
    assert!(error.to_string().contains("unable to read"));
    assert_eq!(
      tokio::fs::read(&partial)
        .await
        .expect("read resumable partial"),
      b"pay"
    );

    let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind resume fixture server");
    let second_address = second_listener.local_addr().expect("read resume address");
    let second_server = tokio::spawn(async move {
      let (mut stream, _) = second_listener
        .accept()
        .await
        .expect("accept resume fixture request");
      let mut request = [0_u8; 1024];
      let count = tokio::io::AsyncReadExt::read(&mut stream, &mut request)
        .await
        .expect("read resume fixture request");
      tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 3-6/7\r\nConnection: close\r\n\r\nload",
      )
      .await
      .expect("write resume fixture response");
      String::from_utf8_lossy(&request[..count]).into_owned()
    });
    release.url = format!("http://{second_address}/runtime.tar.gz");
    let mut progress = Vec::new();
    download_runtime_artifact(
      &client,
      &release,
      &partial,
      &target,
      &mut |downloaded, _| progress.push(downloaded),
      &|| false,
    )
    .await
    .expect("resume Runtime fixture");
    let request = second_server.await.expect("join resume fixture server");

    assert!(request
      .lines()
      .any(|line| line.eq_ignore_ascii_case("range: bytes=3-")));
    assert_eq!(progress.first(), Some(&3));
    assert_eq!(progress.last(), Some(&7));
    assert_eq!(
      tokio::fs::read(&target).await.expect("read resumed target"),
      payload
    );
    assert!(!partial.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn restarts_safely_when_a_runtime_server_ignores_range() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind fallback fixture server");
    let address = listener.local_addr().expect("read fixture address");
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.expect("accept fixture request");
      let mut request = [0_u8; 1024];
      let count = tokio::io::AsyncReadExt::read(&mut stream, &mut request)
        .await
        .expect("read fallback fixture request");
      tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\npayload",
      )
      .await
      .expect("write fallback fixture response");
      String::from_utf8_lossy(&request[..count]).into_owned()
    });

    let root = std::env::temp_dir().join(format!("fabdev-runtime-fallback-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create fallback fixture");
    let partial = root.join("runtime.tar.gz.part");
    let target = root.join("runtime.tar.gz");
    tokio::fs::write(&partial, b"pay")
      .await
      .expect("write fallback partial");
    let mut release = catalog().runtimes.remove(0);
    release.url = format!("http://{address}/runtime.tar.gz");
    let client = Client::builder().build().expect("build fixture client");

    download_runtime_artifact(
      &client,
      &release,
      &partial,
      &target,
      &mut |_, _| {},
      &|| false,
    )
    .await
    .expect("restart Runtime fixture");
    let request = server.await.expect("join fallback fixture server");

    assert!(request
      .lines()
      .any(|line| line.eq_ignore_ascii_case("range: bytes=3-")));
    assert_eq!(
      tokio::fs::read(&target)
        .await
        .expect("read fallback target"),
      b"payload"
    );
    assert!(!partial.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[cfg(target_os = "macos")]
  #[tokio::test]
  #[ignore = "requires a verified macOS PHP Runtime package"]
  async fn streams_the_real_macos_php_package_over_loopback() {
    let artifact = PathBuf::from(
      std::env::var("FABDEV_MACOS_PHP_RUNTIME_PACKAGE")
        .expect("FABDEV_MACOS_PHP_RUNTIME_PACKAGE must point to the verified package"),
    );
    assert_eq!(
      artifact.file_name().and_then(|name| name.to_str()),
      Some("php-8.4.24-macos-arm64-community.tar.gz")
    );
    let artifact_size = tokio::fs::metadata(&artifact)
      .await
      .expect("read Runtime package metadata")
      .len();
    let artifact_sha256 = hex::encode(Sha256::digest(
      tokio::fs::read(&artifact)
        .await
        .expect("read Runtime package"),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind fixture server");
    let address = listener.local_addr().expect("read fixture address");
    let server_artifact = artifact.clone();
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.expect("accept fixture request");
      let mut request = [0_u8; 4096];
      let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut request)
        .await
        .expect("read fixture request");
      tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        format!("HTTP/1.1 200 OK\r\nContent-Length: {artifact_size}\r\nConnection: close\r\n\r\n")
          .as_bytes(),
      )
      .await
      .expect("write fixture response headers");

      let mut file = tokio::fs::File::open(server_artifact)
        .await
        .expect("open fixture Runtime package");
      let mut buffer = vec![0_u8; 64 * 1024];
      loop {
        let count = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
          .await
          .expect("read fixture Runtime package");
        if count == 0 {
          break;
        }
        tokio::io::AsyncWriteExt::write_all(&mut stream, &buffer[..count])
          .await
          .expect("stream fixture Runtime package");
      }
    });

    let root = std::env::temp_dir().join(format!("fabdev-runtime-real-network-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create fixture");
    let partial = root.join("php-runtime.tar.gz.part");
    let target = root.join("php-runtime.tar.gz");
    let mut release = catalog().runtimes.remove(0);
    release.url = format!("http://{address}/php-runtime.tar.gz");
    release.size = artifact_size;
    release.sha256 = artifact_sha256;
    let progress = Arc::new(AtomicU64::new(0));
    let reported = Arc::clone(&progress);
    let client = Client::builder().build().expect("build fixture client");

    download_runtime_artifact(
      &client,
      &release,
      &partial,
      &target,
      &mut |downloaded, _| reported.store(downloaded, Ordering::Relaxed),
      &|| false,
    )
    .await
    .expect("download real Runtime fixture");
    server.await.expect("join fixture server");

    assert_eq!(progress.load(Ordering::Relaxed), artifact_size);
    verify_runtime_artifact(&target, &release)
      .await
      .expect("verify downloaded Runtime package");
    assert_eq!(
      tokio::fs::metadata(&target)
        .await
        .expect("read downloaded Runtime package metadata")
        .len(),
      artifact_size
    );
    assert!(!partial.exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn restricts_runtime_redirect_hosts() {
    assert!(is_allowed_runtime_update_host(Some("github.com")));
    assert!(is_allowed_runtime_update_host(Some(
      "release-assets.githubusercontent.com"
    )));
    assert!(!is_allowed_runtime_update_host(Some("example.com")));
    assert!(!is_allowed_runtime_update_host(None));
  }
}
