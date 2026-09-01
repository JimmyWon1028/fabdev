use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context};
use futures_util::StreamExt;
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use fabdev_runtime::{RuntimeCatalog, RuntimeRelease};

mod runtime_updates;
mod windows_download;

pub use runtime_updates::{
  cached_runtime_catalog, check_for_runtime_updates, cleanup_runtime_update_partials,
  download_cached_runtime_update, verified_cached_runtime_update, DownloadedRuntimeUpdate,
  RuntimeDownloadRequest,
};

pub const STABLE_MANIFEST_URL: &str =
  "https://github.com/JimmyWon1028/fabdev/releases/latest/download/fabdev-stable-v1.json";
const RELEASE_BASE_URL: &str = "https://github.com/JimmyWon1028/fabdev/releases";
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const PENDING_DIRECTORY: &str = "updates/pending";
const PENDING_MANIFEST_FILE: &str = "fabdev-app-v1.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppReleaseManifest {
  schema_version: u16,
  product: String,
  channel: String,
  version: String,
  tag: String,
  published_at: String,
  release_url: String,
  release_notes_url: String,
  unsigned_community_build: bool,
  integrity: String,
  compatibility: AppCompatibility,
  artifacts: Vec<AppReleaseArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCompatibility {
  agent_protocol_version: u16,
  requires_full_installer: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppReleaseArtifact {
  platform: String,
  architecture: String,
  minimum_os_version: String,
  file_name: String,
  url: String,
  size: u64,
  sha256: String,
  signature: Option<String>,
  install_mode: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateArtifact {
  pub platform: String,
  pub architecture: String,
  pub minimum_os_version: String,
  pub file_name: String,
  pub size: u64,
  pub sha256: String,
  pub install_mode: String,
}

impl From<&AppReleaseArtifact> for AppUpdateArtifact {
  fn from(artifact: &AppReleaseArtifact) -> Self {
    Self {
      platform: artifact.platform.clone(),
      architecture: artifact.architecture.clone(),
      minimum_os_version: artifact.minimum_os_version.clone(),
      file_name: artifact.file_name.clone(),
      size: artifact.size,
      sha256: artifact.sha256.clone(),
      install_mode: artifact.install_mode.clone(),
    }
  }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheck {
  pub current_version: String,
  pub latest_version: String,
  pub update_available: bool,
  pub published_at: String,
  pub release_url: String,
  pub release_notes_url: String,
  pub unsigned_community_build: bool,
  pub artifact: AppUpdateArtifact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedAppUpdate {
  pub version: String,
  pub file_name: String,
  pub size: u64,
  pub sha256: String,
}

pub fn find_release<'a>(
  catalog: &'a RuntimeCatalog,
  name: &str,
  version: &str,
  platform: &str,
  architecture: &str,
) -> Option<&'a RuntimeRelease> {
  catalog.runtimes.iter().find(|release| {
    release.name == name
      && release.version == version
      && release.platform == platform
      && release.architecture == architecture
  })
}

pub async fn check_for_app_update(
  current_version: &str,
  platform: &str,
  architecture: &str,
) -> anyhow::Result<AppUpdateCheck> {
  let client = http_client()?;
  let manifest = fetch_stable_manifest(&client).await?;
  build_update_check(&manifest, current_version, platform, architecture)
}

pub async fn download_app_update<F>(
  cache_directory: &Path,
  current_version: &str,
  platform: &str,
  architecture: &str,
  on_progress: F,
) -> anyhow::Result<DownloadedAppUpdate>
where
  F: FnMut(u64, u64),
{
  download_app_update_with_cancellation(
    cache_directory,
    current_version,
    platform,
    architecture,
    on_progress,
    || false,
  )
  .await
}

pub async fn download_app_update_with_cancellation<F, C>(
  cache_directory: &Path,
  current_version: &str,
  platform: &str,
  architecture: &str,
  mut on_progress: F,
  is_cancelled: C,
) -> anyhow::Result<DownloadedAppUpdate>
where
  F: FnMut(u64, u64),
  C: Fn() -> bool + Sync,
{
  let client = http_client()?;
  let manifest = fetch_stable_manifest(&client).await?;
  let check = build_update_check(&manifest, current_version, platform, architecture)?;
  if !check.update_available {
    bail!("no newer stable fabDev version is available");
  }
  let artifact = select_artifact(&manifest, platform, architecture)?;
  let pending_directory = cache_directory.join(PENDING_DIRECTORY);
  tokio::fs::create_dir_all(&pending_directory)
    .await
    .context("unable to create the app update download directory")?;
  let target = pending_directory.join(&artifact.file_name);

  if target.is_file() && verify_artifact(&target, artifact).await.is_ok() {
    write_pending_manifest(&pending_directory, &manifest).await?;
    on_progress(artifact.size, artifact.size);
    return Ok(downloaded_update(&manifest, artifact));
  }

  remove_file_if_exists(&target).await?;
  let partial = pending_directory.join(format!("{}.part", artifact.file_name));
  remove_file_if_exists(&partial).await?;
  on_progress(0, artifact.size);

  let windows_x64 = platform == "windows" && architecture == "x64";
  let result = if windows_x64 {
    windows_download::download_windows_artifact(
      windows_download::WindowsArtifactDownload {
        client: &client,
        url: &artifact.url,
        size: artifact.size,
        sha256: &artifact.sha256,
        partial: &partial,
        target: &target,
      },
      &mut on_progress,
      &is_cancelled,
    )
    .await
  } else {
    download_artifact(&client, artifact, &partial, &target, &mut on_progress).await
  };
  if result.is_err() {
    let _ = remove_file_if_exists(&partial).await;
  }
  result?;
  write_pending_manifest(&pending_directory, &manifest).await?;
  Ok(downloaded_update(&manifest, artifact))
}

pub async fn pending_app_update(
  cache_directory: &Path,
  platform: &str,
  architecture: &str,
) -> anyhow::Result<(DownloadedAppUpdate, PathBuf)> {
  let pending_directory = cache_directory.join(PENDING_DIRECTORY);
  let manifest_path = pending_directory.join(PENDING_MANIFEST_FILE);
  let contents = tokio::fs::read(&manifest_path)
    .await
    .context("no verified app update has been downloaded")?;
  if contents.len() > MAX_MANIFEST_BYTES {
    bail!("cached app update manifest exceeds the size limit");
  }
  let manifest = parse_and_validate_manifest(&contents)?;
  let artifact = select_artifact(&manifest, platform, architecture)?;
  let path = pending_directory.join(&artifact.file_name);
  verify_artifact(&path, artifact).await?;
  Ok((downloaded_update(&manifest, artifact), path))
}

pub fn release_notes_url(version: &str) -> anyhow::Result<String> {
  validate_stable_version(version)?;
  Ok(format!("{RELEASE_BASE_URL}/tag/v{version}"))
}

fn http_client() -> anyhow::Result<Client> {
  Client::builder()
    .user_agent(format!("fabDev/{}", env!("CARGO_PKG_VERSION")))
    .connect_timeout(Duration::from_secs(15))
    .https_only(true)
    .build()
    .context("unable to initialize the app update HTTPS client")
}

async fn fetch_stable_manifest(client: &Client) -> anyhow::Result<AppReleaseManifest> {
  let response = client
    .get(STABLE_MANIFEST_URL)
    .timeout(Duration::from_secs(30))
    .send()
    .await
    .context("unable to download the stable app update manifest")?
    .error_for_status()
    .context("the stable app update manifest returned an unsuccessful status")?;
  if response
    .content_length()
    .is_some_and(|length| length > MAX_MANIFEST_BYTES as u64)
  {
    bail!("stable app update manifest exceeds the size limit");
  }
  let mut contents = Vec::new();
  let mut stream = response.bytes_stream();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.context("unable to read the stable app update manifest")?;
    if contents.len() + chunk.len() > MAX_MANIFEST_BYTES {
      bail!("stable app update manifest exceeds the size limit");
    }
    contents.extend_from_slice(&chunk);
  }
  parse_and_validate_manifest(&contents)
}

fn parse_and_validate_manifest(contents: &[u8]) -> anyhow::Result<AppReleaseManifest> {
  let manifest: AppReleaseManifest =
    serde_json::from_slice(contents).context("stable app update manifest is invalid JSON")?;
  validate_manifest(&manifest)?;
  Ok(manifest)
}

fn validate_manifest(manifest: &AppReleaseManifest) -> anyhow::Result<()> {
  if manifest.schema_version != 1 {
    bail!("unsupported app update manifest schema version");
  }
  if manifest.product != "fabdev" || manifest.channel != "stable" {
    bail!("app update manifest does not describe the fabDev stable channel");
  }
  validate_stable_version(&manifest.version)?;
  if manifest.tag != format!("v{}", manifest.version) {
    bail!("app update manifest tag does not match its version");
  }
  let release_url = format!("{RELEASE_BASE_URL}/tag/{}", manifest.tag);
  if manifest.release_url != release_url || manifest.release_notes_url != release_url {
    bail!("app update manifest release URL is not the official fabDev Release URL");
  }
  if !manifest.unsigned_community_build || manifest.integrity != "sha256" {
    bail!("app update manifest has an unsupported signing or integrity mode");
  }
  if !manifest.compatibility.requires_full_installer {
    bail!("app update manifest must require the full installer");
  }
  if manifest.compatibility.agent_protocol_version == 0 {
    bail!("app update manifest has an invalid Agent Protocol version");
  }
  if !is_utc_rfc3339_seconds(&manifest.published_at) {
    bail!("app update manifest publish time is not UTC RFC 3339 seconds");
  }
  if manifest.artifacts.is_empty() {
    bail!("app update manifest does not contain an installer");
  }
  for artifact in &manifest.artifacts {
    validate_artifact(&manifest.version, artifact)?;
  }
  Ok(())
}

fn validate_artifact(version: &str, artifact: &AppReleaseArtifact) -> anyhow::Result<()> {
  let (expected_name, expected_mode) =
    match (artifact.platform.as_str(), artifact.architecture.as_str()) {
      ("macos", "arm64") => (
        format!("fabDev-Community-{version}-macos-arm64.dmg"),
        "open-dmg",
      ),
      ("windows", "x64") => (
        format!("fabDev-Community-{version}-windows-x64-setup.exe"),
        "run-installer-after-quit",
      ),
      _ => bail!("app update manifest contains an unsupported platform or architecture"),
    };
  if artifact.file_name != expected_name || artifact.install_mode != expected_mode {
    bail!("app update artifact name or install mode is invalid");
  }
  let expected_url = format!(
    "{RELEASE_BASE_URL}/download/v{version}/{}",
    artifact.file_name
  );
  if artifact.url != expected_url {
    bail!("app update artifact URL is not the official versioned fabDev download URL");
  }
  if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
    bail!("app update artifact size is invalid");
  }
  if !is_lowercase_sha256(&artifact.sha256) {
    bail!("app update artifact SHA-256 is invalid");
  }
  if artifact.signature.is_some() {
    bail!("unsigned Community app update must not claim a release signature");
  }
  if artifact.minimum_os_version.is_empty() {
    bail!("app update artifact is missing its minimum OS version");
  }
  Ok(())
}

fn validate_stable_version(version: &str) -> anyhow::Result<Version> {
  let version = Version::parse(version).context("app update version is not valid SemVer")?;
  if !version.pre.is_empty() || !version.build.is_empty() {
    bail!("stable app update version must not contain prerelease or build metadata");
  }
  Ok(version)
}

fn build_update_check(
  manifest: &AppReleaseManifest,
  current_version: &str,
  platform: &str,
  architecture: &str,
) -> anyhow::Result<AppUpdateCheck> {
  let current =
    Version::parse(current_version).context("current app version is not valid SemVer")?;
  let latest = validate_stable_version(&manifest.version)?;
  let artifact = select_artifact(manifest, platform, architecture)?;
  Ok(AppUpdateCheck {
    current_version: current_version.to_owned(),
    latest_version: manifest.version.clone(),
    update_available: latest > current,
    published_at: manifest.published_at.clone(),
    release_url: manifest.release_url.clone(),
    release_notes_url: manifest.release_notes_url.clone(),
    unsigned_community_build: manifest.unsigned_community_build,
    artifact: artifact.into(),
  })
}

fn select_artifact<'a>(
  manifest: &'a AppReleaseManifest,
  platform: &str,
  architecture: &str,
) -> anyhow::Result<&'a AppReleaseArtifact> {
  let mut matches = manifest
    .artifacts
    .iter()
    .filter(|artifact| artifact.platform == platform && artifact.architecture == architecture);
  let artifact = matches
    .next()
    .context("stable app update does not include an installer for this platform")?;
  if matches.next().is_some() {
    bail!("stable app update contains duplicate installers for this platform");
  }
  Ok(artifact)
}

async fn download_artifact<F>(
  client: &Client,
  artifact: &AppReleaseArtifact,
  partial: &Path,
  target: &Path,
  on_progress: &mut F,
) -> anyhow::Result<()>
where
  F: FnMut(u64, u64),
{
  let response = client
    .get(&artifact.url)
    .send()
    .await
    .context("unable to download the app update installer")?
    .error_for_status()
    .context("the app update installer returned an unsuccessful status")?;
  if response
    .content_length()
    .is_some_and(|length| length != artifact.size)
  {
    bail!("app update installer size does not match the manifest");
  }

  let mut file = tokio::fs::File::create(partial)
    .await
    .context("unable to create the partial app update file")?;
  let mut hasher = Sha256::new();
  let mut downloaded = 0_u64;
  let mut stream = response.bytes_stream();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.context("unable to read the app update installer download")?;
    downloaded = downloaded
      .checked_add(chunk.len() as u64)
      .context("app update installer size overflow")?;
    if downloaded > artifact.size {
      bail!("app update installer exceeds the manifest size");
    }
    file
      .write_all(&chunk)
      .await
      .context("unable to write the app update installer")?;
    hasher.update(&chunk);
    on_progress(downloaded, artifact.size);
  }
  file
    .flush()
    .await
    .context("unable to flush the app update installer")?;
  file
    .sync_all()
    .await
    .context("unable to sync the app update installer")?;
  drop(file);

  if downloaded != artifact.size {
    bail!("app update installer download is incomplete");
  }
  let checksum = hex::encode(hasher.finalize());
  if checksum != artifact.sha256 {
    bail!("app update installer SHA-256 does not match the manifest");
  }
  tokio::fs::rename(partial, target)
    .await
    .context("unable to finalize the verified app update installer")?;
  Ok(())
}

async fn verify_artifact(path: &Path, artifact: &AppReleaseArtifact) -> anyhow::Result<()> {
  let metadata = tokio::fs::metadata(path)
    .await
    .context("verified app update installer is missing")?;
  if !metadata.is_file() || metadata.len() != artifact.size {
    bail!("downloaded app update installer size does not match the manifest");
  }
  let mut file = tokio::fs::File::open(path)
    .await
    .context("unable to open the downloaded app update installer")?;
  let mut buffer = vec![0_u8; 64 * 1024];
  let mut hasher = Sha256::new();
  loop {
    let count = file
      .read(&mut buffer)
      .await
      .context("unable to verify the downloaded app update installer")?;
    if count == 0 {
      break;
    }
    hasher.update(&buffer[..count]);
  }
  if hex::encode(hasher.finalize()) != artifact.sha256 {
    bail!("downloaded app update installer SHA-256 does not match the manifest");
  }
  Ok(())
}

async fn write_pending_manifest(
  pending_directory: &Path,
  manifest: &AppReleaseManifest,
) -> anyhow::Result<()> {
  let mut contents = serde_json::to_vec_pretty(manifest)
    .context("unable to serialize the verified app update manifest")?;
  contents.push(b'\n');
  let target = pending_directory.join(PENDING_MANIFEST_FILE);
  let partial = pending_directory.join(format!("{PENDING_MANIFEST_FILE}.part"));
  remove_file_if_exists(&partial).await?;
  let mut file = tokio::fs::File::create(&partial)
    .await
    .context("unable to create the pending app update manifest")?;
  file
    .write_all(&contents)
    .await
    .context("unable to write the pending app update manifest")?;
  file
    .sync_all()
    .await
    .context("unable to sync the pending app update manifest")?;
  drop(file);
  remove_file_if_exists(&target).await?;
  tokio::fs::rename(partial, target)
    .await
    .context("unable to finalize the pending app update manifest")
}

async fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
  match tokio::fs::remove_file(path).await {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn downloaded_update(
  manifest: &AppReleaseManifest,
  artifact: &AppReleaseArtifact,
) -> DownloadedAppUpdate {
  DownloadedAppUpdate {
    version: manifest.version.clone(),
    file_name: artifact.file_name.clone(),
    size: artifact.size,
    sha256: artifact.sha256.clone(),
  }
}

fn is_lowercase_sha256(value: &str) -> bool {
  value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_utc_rfc3339_seconds(value: &str) -> bool {
  let bytes = value.as_bytes();
  if bytes.len() != 20
    || bytes[4] != b'-'
    || bytes[7] != b'-'
    || bytes[10] != b'T'
    || bytes[13] != b':'
    || bytes[16] != b':'
    || bytes[19] != b'Z'
  {
    return false;
  }
  if bytes
    .iter()
    .enumerate()
    .any(|(index, byte)| !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit())
  {
    return false;
  }
  let number = |start: usize, end: usize| {
    value[start..end]
      .parse::<u16>()
      .expect("timestamp digits were validated")
  };
  let month = number(5, 7);
  let day = number(8, 10);
  let hour = number(11, 13);
  let minute = number(14, 16);
  let second = number(17, 19);
  (1..=12).contains(&month) && (1..=31).contains(&day) && hour <= 23 && minute <= 59 && second <= 59
}

#[cfg(test)]
mod tests {
  use super::*;
  use uuid::Uuid;

  fn manifest_json(version: &str, sha256: &str) -> Vec<u8> {
    format!(
      r#"{{
        "schemaVersion": 1,
        "product": "fabdev",
        "channel": "stable",
        "version": "{version}",
        "tag": "v{version}",
        "publishedAt": "2026-08-29T00:00:00Z",
        "releaseUrl": "https://github.com/JimmyWon1028/fabdev/releases/tag/v{version}",
        "releaseNotesUrl": "https://github.com/JimmyWon1028/fabdev/releases/tag/v{version}",
        "unsignedCommunityBuild": true,
        "integrity": "sha256",
        "compatibility": {{
          "agentProtocolVersion": 32,
          "requiresFullInstaller": true
        }},
        "artifacts": [
          {{
            "platform": "macos",
            "architecture": "arm64",
            "minimumOsVersion": "13.0",
            "fileName": "fabDev-Community-{version}-macos-arm64.dmg",
            "url": "https://github.com/JimmyWon1028/fabdev/releases/download/v{version}/fabDev-Community-{version}-macos-arm64.dmg",
            "size": 7,
            "sha256": "{sha256}",
            "signature": null,
            "installMode": "open-dmg"
          }}
        ]
      }}"#
    )
    .into_bytes()
  }

  #[test]
  fn filters_runtime_release_by_platform_and_architecture() {
    let catalog = RuntimeCatalog {
      schema_version: 1,
      product: "fabdev-runtime".to_owned(),
      channel: "community".to_owned(),
      catalog_sequence: 1,
      generated_at: "2026-08-22T00:00:00Z".to_owned(),
      expires_at: "2027-08-22T00:00:00Z".to_owned(),
      unsigned_community_build: true,
      integrity: "sha256".to_owned(),
      compatibility: fabdev_runtime::RuntimeCatalogCompatibility {
        minimum_app_version: "0.1.4".to_owned(),
        minimum_agent_protocol_version: 33,
      },
      signature: None,
      runtimes: vec![RuntimeRelease {
        name: "php".to_owned(),
        version: "8.2.33".to_owned(),
        platform: "macos".to_owned(),
        architecture: "arm64".to_owned(),
        url: "https://example.invalid/php.tar.zst".to_owned(),
        size: 1,
        sha256: "00".repeat(32),
        signature: Some("development".to_owned()),
        ..RuntimeRelease::default()
      }],
    };
    assert!(find_release(&catalog, "php", "8.2.33", "macos", "arm64").is_some());
    assert!(find_release(&catalog, "php", "8.2.33", "windows", "x64").is_none());
  }

  #[test]
  fn validates_manifest_and_compares_versions() {
    let contents = manifest_json("0.1.2", &"a".repeat(64));
    let manifest = parse_and_validate_manifest(&contents).expect("validate manifest");
    let check =
      build_update_check(&manifest, "0.1.1", "macos", "arm64").expect("build update check");
    assert!(check.update_available);
    assert_eq!(check.latest_version, "0.1.2");
    assert_eq!(
      check.artifact.file_name,
      "fabDev-Community-0.1.2-macos-arm64.dmg"
    );

    let same_version =
      build_update_check(&manifest, "0.1.2", "macos", "arm64").expect("build same-version check");
    assert!(!same_version.update_available);
  }

  #[test]
  fn rejects_unofficial_artifact_url_and_claimed_signature() {
    let mut manifest = parse_and_validate_manifest(&manifest_json("0.1.2", &"a".repeat(64)))
      .expect("validate fixture");
    manifest.artifacts[0].url = "https://example.invalid/fabdev.dmg".to_owned();
    assert!(validate_manifest(&manifest)
      .expect_err("reject unofficial URL")
      .to_string()
      .contains("official versioned"));

    manifest.artifacts[0].url = format!(
      "{RELEASE_BASE_URL}/download/v0.1.2/{}",
      manifest.artifacts[0].file_name
    );
    manifest.artifacts[0].signature = Some("community-ad-hoc".to_owned());
    assert!(validate_manifest(&manifest)
      .expect_err("reject claimed signature")
      .to_string()
      .contains("must not claim"));
  }

  #[test]
  fn rejects_invalid_publish_time() {
    let mut manifest = parse_and_validate_manifest(&manifest_json("0.1.2", &"a".repeat(64)))
      .expect("validate fixture");
    manifest.published_at = "2026-08-29".to_owned();
    assert!(validate_manifest(&manifest)
      .expect_err("reject invalid publish time")
      .to_string()
      .contains("UTC RFC 3339"));
  }

  #[tokio::test]
  async fn verifies_cached_installer_size_and_sha256() {
    let root = std::env::temp_dir().join(format!("fabdev-update-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create update fixture");
    let path = root.join("fixture.dmg");
    tokio::fs::write(&path, b"fixture")
      .await
      .expect("write installer fixture");
    let artifact = AppReleaseArtifact {
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      minimum_os_version: "13.0".to_owned(),
      file_name: "fixture.dmg".to_owned(),
      url: "https://example.invalid/fixture.dmg".to_owned(),
      size: 7,
      sha256: hex::encode(Sha256::digest(b"fixture")),
      signature: None,
      install_mode: "open-dmg".to_owned(),
    };
    verify_artifact(&path, &artifact)
      .await
      .expect("verify installer fixture");
    tokio::fs::write(&path, b"changed")
      .await
      .expect("change installer fixture");
    assert!(verify_artifact(&path, &artifact).await.is_err());
    let _ = tokio::fs::remove_dir_all(root).await;
  }

  #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
  #[tokio::test]
  #[ignore = "requires GitHub Releases network access"]
  async fn reads_the_public_stable_manifest() {
    let check = check_for_app_update("0.1.1", "macos", "arm64")
      .await
      .expect("read public stable manifest");
    assert_eq!(check.current_version, "0.1.1");
    assert_eq!(check.artifact.platform, "macos");
    assert_eq!(check.artifact.architecture, "arm64");
  }

  #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
  #[tokio::test]
  #[ignore = "requires GitHub Releases network access and downloads the current DMG"]
  async fn downloads_and_verifies_the_public_stable_macos_installer() {
    let root = std::env::temp_dir().join(format!("fabdev-public-update-{}", Uuid::new_v4()));
    let mut last_progress = (0, 0);
    let download = download_app_update(&root, "0.0.0", "macos", "arm64", |downloaded, total| {
      last_progress = (downloaded, total);
    })
    .await
    .expect("download public stable installer");
    assert_eq!(last_progress, (download.size, download.size));
    let (pending, path) = pending_app_update(&root, "macos", "arm64")
      .await
      .expect("load verified pending installer");
    assert_eq!(pending, download);
    assert!(path.is_file());
    let _ = tokio::fs::remove_dir_all(root).await;
  }
}
