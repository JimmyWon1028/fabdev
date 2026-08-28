use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCatalog {
  pub schema_version: u16,
  pub generated_at: String,
  pub runtimes: Vec<RuntimeRelease>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRelease {
  pub name: String,
  pub version: String,
  pub platform: String,
  pub architecture: String,
  pub url: String,
  pub size: u64,
  pub sha256: String,
  pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallLayout {
  pub runtime_root: PathBuf,
  pub staging_root: PathBuf,
  pub active_link: PathBuf,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
  #[error("unable to read runtime artifact: {0}")]
  Read(#[from] std::io::Error),
  #[error("runtime checksum mismatch: expected {expected}, got {actual}")]
  ChecksumMismatch { expected: String, actual: String },
  #[error("runtime is already installed: {0}")]
  AlreadyInstalled(PathBuf),
  #[error("runtime archive does not contain the expected top-level directory: {0}")]
  InvalidArchive(String),
  #[error("invalid runtime identifier: {0}")]
  InvalidIdentifier(String),
  #[error("runtime is not installed: {0} {1}")]
  NotInstalled(String, String),
  #[error("cannot remove the active runtime: {0} {1}")]
  ActiveRuntime(String, String),
  #[error("invalid active runtime link: {0}")]
  InvalidActiveLink(PathBuf),
  #[error("runtime installation is not supported on this platform")]
  UnsupportedPlatform,
}

impl InstallLayout {
  pub fn new(base: impl AsRef<Path>, name: &str, version: &str) -> Self {
    let runtime_root = base.as_ref().join(name).join(version);
    #[cfg(unix)]
    let active_link = base.as_ref().join(name).join("current");
    #[cfg(windows)]
    let active_link = base.as_ref().join(name).join("current.version");
    Self {
      staging_root: base
        .as_ref()
        .join(".staging")
        .join(format!("{name}-{version}")),
      active_link,
      runtime_root,
    }
  }
}

pub fn verify_sha256(path: impl AsRef<Path>, expected: &str) -> Result<(), RuntimeError> {
  let file = File::open(path)?;
  let mut reader = BufReader::new(file);
  let mut hasher = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = reader.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    hasher.update(&buffer[..count]);
  }
  let actual = hex::encode(hasher.finalize());
  if actual.eq_ignore_ascii_case(expected) {
    Ok(())
  } else {
    Err(RuntimeError::ChecksumMismatch {
      expected: expected.to_owned(),
      actual,
    })
  }
}

pub fn install_tar_gz(
  artifact: impl AsRef<Path>,
  expected_sha256: &str,
  name: &str,
  version: &str,
  base: impl AsRef<Path>,
) -> Result<InstallLayout, RuntimeError> {
  install_tar_gz_with_activation(artifact, expected_sha256, name, version, base, true)
}

pub fn install_tar_gz_with_activation(
  artifact: impl AsRef<Path>,
  expected_sha256: &str,
  name: &str,
  version: &str,
  base: impl AsRef<Path>,
  activate: bool,
) -> Result<InstallLayout, RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  verify_sha256(&artifact, expected_sha256)?;
  let base = base.as_ref();
  let layout = InstallLayout::new(base, name, version);
  if layout.runtime_root.exists() {
    return Err(RuntimeError::AlreadyInstalled(layout.runtime_root));
  }
  remove_dir_if_exists(&layout.staging_root)?;
  std::fs::create_dir_all(&layout.staging_root)?;

  let archive = File::open(artifact)?;
  let mut archive = tar::Archive::new(GzDecoder::new(BufReader::new(archive)));
  archive.unpack(&layout.staging_root)?;
  let extracted = layout.staging_root.join(version);
  if !extracted.is_dir() {
    remove_dir_if_exists(&layout.staging_root)?;
    return Err(RuntimeError::InvalidArchive(version.to_owned()));
  }

  let runtime_parent = layout
    .runtime_root
    .parent()
    .ok_or_else(|| RuntimeError::InvalidArchive(name.to_owned()))?;
  std::fs::create_dir_all(runtime_parent)?;
  std::fs::rename(&extracted, &layout.runtime_root)?;
  remove_dir_if_exists(&layout.staging_root)?;
  if activate {
    switch_current(runtime_parent, version, &layout.active_link)?;
  }
  clear_runtime_removal_marker(base, name, version)?;
  Ok(layout)
}

pub fn mark_runtime_removed(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<(), RuntimeError> {
  let marker = runtime_removal_marker(base, name, version)?;
  if let Some(parent) = marker.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(marker, b"removed\n")?;
  Ok(())
}

pub fn clear_runtime_removal_marker(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<(), RuntimeError> {
  let marker = runtime_removal_marker(base, name, version)?;
  match std::fs::remove_file(marker) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

pub fn is_runtime_marked_removed(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<bool, RuntimeError> {
  Ok(runtime_removal_marker(base, name, version)?.is_file())
}

fn runtime_removal_marker(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<PathBuf, RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  Ok(base.as_ref().join(".removed").join(name).join(version))
}

pub fn list_installed_versions(
  base: impl AsRef<Path>,
  name: &str,
) -> Result<Vec<String>, RuntimeError> {
  validate_identifier(name)?;
  let parent = base.as_ref().join(name);
  let entries = match std::fs::read_dir(parent) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(error) => return Err(error.into()),
  };
  let mut versions = entries
    .filter_map(|entry| entry.ok())
    .filter_map(|entry| {
      let file_type = entry.file_type().ok()?;
      let name = entry.file_name().into_string().ok()?;
      (file_type.is_dir() && validate_identifier(&name).is_ok()).then_some(name)
    })
    .collect::<Vec<_>>();
  versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
  Ok(versions)
}

pub fn active_version(base: impl AsRef<Path>, name: &str) -> Result<Option<String>, RuntimeError> {
  validate_identifier(name)?;
  #[cfg(unix)]
  let active_link = base.as_ref().join(name).join("current");
  #[cfg(windows)]
  let active_link = base.as_ref().join(name).join("current.version");

  #[cfg(windows)]
  {
    let version = match std::fs::read_to_string(&active_link) {
      Ok(version) => version.trim().to_owned(),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(error) => return Err(error.into()),
    };
    if validate_identifier(&version).is_err() || !base.as_ref().join(name).join(&version).is_dir() {
      return Err(RuntimeError::InvalidActiveLink(active_link));
    }
    return Ok(Some(version));
  }

  #[cfg(unix)]
  {
    let target = match std::fs::read_link(&active_link) {
      Ok(target) => target,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(error) => return Err(error.into()),
    };
    if target.components().count() != 1 {
      return Err(RuntimeError::InvalidActiveLink(active_link));
    }
    let version = target
      .to_str()
      .filter(|version| validate_identifier(version).is_ok())
      .ok_or_else(|| RuntimeError::InvalidActiveLink(active_link.clone()))?;
    if !base.as_ref().join(name).join(version).is_dir() {
      return Err(RuntimeError::InvalidActiveLink(active_link));
    }
    Ok(Some(version.to_owned()))
  }
}

pub fn set_active_version(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<(), RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  let parent = base.as_ref().join(name);
  if !parent.join(version).is_dir() {
    return Err(RuntimeError::NotInstalled(
      name.to_owned(),
      version.to_owned(),
    ));
  }
  #[cfg(unix)]
  let active_link = parent.join("current");
  #[cfg(windows)]
  let active_link = parent.join("current.version");
  switch_current(&parent, version, &active_link)
}

pub fn deactivate_runtime(
  base: impl AsRef<Path>,
  name: &str,
) -> Result<Option<String>, RuntimeError> {
  validate_identifier(name)?;
  let version = active_version(&base, name)?;
  if version.is_none() {
    return Ok(None);
  }
  #[cfg(unix)]
  let active_link = base.as_ref().join(name).join("current");
  #[cfg(windows)]
  let active_link = base.as_ref().join(name).join("current.version");
  std::fs::remove_file(active_link)?;
  Ok(version)
}

pub fn remove_installed_version(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<(), RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  if active_version(&base, name)?.as_deref() == Some(version) {
    return Err(RuntimeError::ActiveRuntime(
      name.to_owned(),
      version.to_owned(),
    ));
  }
  let runtime_root = base.as_ref().join(name).join(version);
  let metadata = match std::fs::symlink_metadata(&runtime_root) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Err(RuntimeError::NotInstalled(
        name.to_owned(),
        version.to_owned(),
      ));
    }
    Err(error) => return Err(error.into()),
  };
  if !metadata.is_dir() || metadata.file_type().is_symlink() {
    return Err(RuntimeError::NotInstalled(
      name.to_owned(),
      version.to_owned(),
    ));
  }
  std::fs::remove_dir_all(runtime_root)?;
  Ok(())
}

fn validate_identifier(value: &str) -> Result<(), RuntimeError> {
  let valid = !value.is_empty()
    && !value.starts_with('.')
    && value
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'));
  if valid {
    Ok(())
  } else {
    Err(RuntimeError::InvalidIdentifier(value.to_owned()))
  }
}

fn version_key(value: &str) -> Vec<u64> {
  value
    .split('.')
    .map(|part| part.parse::<u64>().unwrap_or_default())
    .collect()
}

fn remove_dir_if_exists(path: &Path) -> Result<(), RuntimeError> {
  match std::fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(unix)]
fn switch_current(parent: &Path, version: &str, active_link: &Path) -> Result<(), RuntimeError> {
  use std::os::unix::fs::symlink;

  let pending = parent.join(".current.pending");
  match std::fs::remove_file(&pending) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  symlink(version, &pending)?;
  std::fs::rename(pending, active_link)?;
  Ok(())
}

#[cfg(windows)]
fn switch_current(parent: &Path, version: &str, active_link: &Path) -> Result<(), RuntimeError> {
  let pending = parent.join(".current.pending");
  match std::fs::remove_file(&pending) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  std::fs::write(&pending, format!("{version}\n"))?;
  match std::fs::remove_file(active_link) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  std::fs::rename(pending, active_link)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::io::Write;

  use super::*;

  #[test]
  fn verifies_known_checksum() {
    let path = std::env::temp_dir().join("fabdev-runtime-checksum.txt");
    let mut file = File::create(&path).expect("create fixture");
    file.write_all(b"fabdev").expect("write fixture");
    verify_sha256(
      &path,
      "967bf27930d65ef34a9264091b0d49213facb9d1977b1bc0bbd8424429e8e579",
    )
    .expect("verify checksum");
    std::fs::remove_file(path).expect("remove fixture");
  }

  #[test]
  fn creates_atomic_install_layout() {
    let layout = InstallLayout::new("/tmp/runtimes", "php", "8.2.33");
    assert_eq!(
      layout.runtime_root,
      PathBuf::from("/tmp/runtimes/php/8.2.33")
    );
    assert_eq!(
      layout.active_link,
      PathBuf::from("/tmp/runtimes/php").join(if cfg!(windows) {
        "current.version"
      } else {
        "current"
      })
    );
  }

  #[test]
  fn rejects_install_with_wrong_checksum() {
    let root =
      std::env::temp_dir().join(format!("fabdev-runtime-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture");
    let artifact = root.join("runtime.tar.gz");
    std::fs::write(&artifact, b"invalid archive").expect("write fixture");
    let error = install_tar_gz(&artifact, "0000", "php", "8.2.33", root.join("runtimes"))
      .expect_err("reject checksum");
    assert!(matches!(error, RuntimeError::ChecksumMismatch { .. }));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn lists_switches_and_removes_inactive_versions() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-state-{}", uuid::Uuid::new_v4()));
    let php = root.join("php");
    std::fs::create_dir_all(php.join("7.4.33")).expect("create PHP 7.4 fixture");
    std::fs::create_dir_all(php.join("8.2.33")).expect("create PHP 8.2 fixture");

    set_active_version(&root, "php", "8.2.33").expect("activate PHP 8.2");
    assert_eq!(
      list_installed_versions(&root, "php").expect("list versions"),
      vec!["8.2.33", "7.4.33"]
    );
    assert_eq!(
      active_version(&root, "php").expect("read active version"),
      Some("8.2.33".to_owned())
    );
    remove_installed_version(&root, "php", "7.4.33").expect("remove inactive PHP");
    assert!(!php.join("7.4.33").exists());

    let error =
      remove_installed_version(&root, "php", "8.2.33").expect_err("reject active removal");
    assert!(matches!(error, RuntimeError::ActiveRuntime(_, _)));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn deactivates_runtime_without_removing_its_files() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-deactivate-{}",
      uuid::Uuid::new_v4()
    ));
    let runtime = root.join("mariadb/12.3.2");
    std::fs::create_dir_all(&runtime).expect("create MariaDB fixture");
    set_active_version(&root, "mariadb", "12.3.2").expect("activate MariaDB");

    assert_eq!(
      deactivate_runtime(&root, "mariadb").expect("deactivate MariaDB"),
      Some("12.3.2".to_owned())
    );
    assert_eq!(
      active_version(&root, "mariadb").expect("read deactivated Runtime"),
      None
    );
    assert!(runtime.is_dir());
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[test]
  fn rejects_path_components_as_identifiers() {
    let error =
      set_active_version("/tmp/runtimes", "php", "../8.2.33").expect_err("reject traversal");
    assert!(matches!(error, RuntimeError::InvalidIdentifier(_)));
  }

  #[test]
  fn persists_and_clears_explicit_runtime_removal_markers() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-removal-marker-{}",
      uuid::Uuid::new_v4()
    ));
    assert!(!is_runtime_marked_removed(&root, "php", "8.2.33").expect("read empty marker"));
    mark_runtime_removed(&root, "php", "8.2.33").expect("mark Runtime removed");
    assert!(is_runtime_marked_removed(&root, "php", "8.2.33").expect("read marker"));
    clear_runtime_removal_marker(&root, "php", "8.2.33").expect("clear marker");
    assert!(!is_runtime_marked_removed(&root, "php", "8.2.33").expect("read cleared marker"));
    std::fs::remove_dir_all(root).expect("remove marker fixture");
  }
}
