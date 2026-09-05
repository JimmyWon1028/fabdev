use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const RUNTIME_CATALOG_MAX_BYTES: usize = 1024 * 1024;
pub const RUNTIME_CATALOG_PRODUCT: &str = "fabdev-runtime";
pub const RUNTIME_CATALOG_CHANNEL: &str = "community";
pub const RUNTIME_CATALOG_SCHEMA_VERSION: u16 = 1;
pub const RUNTIME_CATALOG_SCHEMA_VERSION_V2: u16 = 2;
pub const RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION: u16 = 33;
pub const COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION: u16 = 36;
pub const COMMUNITY_RUNTIME_CATALOG_V2_MINIMUM_PROTOCOL_VERSION: u16 = 37;
pub const RUNTIME_CATALOG_V1_URL: &str =
  "https://github.com/JimmyWon1028/fabdev/releases/latest/download/fabdev-runtime-v1.json";
pub const RUNTIME_CATALOG_V2_URL: &str =
  "https://github.com/JimmyWon1028/fabdev-runtimes/releases/latest/download/fabdev-runtime-v2.json";
pub const RUNTIME_CATALOG_URL: &str = RUNTIME_CATALOG_V1_URL;

const RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/JimmyWon1028/fabdev/releases/download/v";
const RUNTIME_RELEASE_DOWNLOAD_PREFIX: &str =
  "https://github.com/JimmyWon1028/fabdev-runtimes/releases/download/catalog-v";
const MAX_GENERATED_AT_FUTURE_SECONDS: i64 = 5 * 60;
const MAX_RUNTIME_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
const RUNTIME_PACKAGE_RECEIPT_FILE: &str = ".fabdev-package.json";
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCatalog {
  pub schema_version: u16,
  pub product: String,
  pub channel: String,
  pub catalog_sequence: u64,
  pub generated_at: String,
  pub expires_at: String,
  pub unsigned_community_build: bool,
  pub integrity: String,
  pub compatibility: RuntimeCatalogCompatibility,
  pub signature: Option<String>,
  pub runtimes: Vec<RuntimeRelease>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCatalogIndex {
  pub schema_version: u16,
  pub runtimes: Vec<RuntimeRelease>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCatalogCompatibility {
  pub minimum_app_version: String,
  pub minimum_agent_protocol_version: u16,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRelease {
  pub name: String,
  pub version: String,
  pub platform: String,
  pub architecture: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub minimum_os_version: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub file_name: Option<String>,
  pub url: String,
  pub size: u64,
  pub sha256: String,
  #[serde(default)]
  pub signature: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source_verification: Option<RuntimeSourceVerification>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub archive_format: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub install_mode: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub health_check_profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSourceVerification {
  pub method: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fingerprint: Option<String>,
  pub upstream_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePackageManifest {
  pub schema_version: u16,
  pub platform: String,
  pub architecture: String,
  pub minimum_os_version: String,
  pub packages: Vec<RuntimePackageDefinition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePackageDefinition {
  pub name: String,
  pub version: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub minimum_os_version: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub build_profile: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub default: Option<bool>,
  pub source: RuntimePackageSource,
  pub health_check_profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePackageSource {
  pub archive_url: String,
  pub archive_sha256: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub signature_url: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub signed_checksums_url: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub key_url: Option<String>,
  pub verification: RuntimePackageVerification,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePackageVerification {
  pub method: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRuntimeCatalog {
  pub schema_version: u16,
  pub sequence: u64,
  pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeCatalogValidation<'a> {
  pub current_app_version: &'a str,
  pub current_agent_protocol_version: u16,
  pub now_unix_seconds: i64,
  pub accepted_catalog: Option<&'a AcceptedRuntimeCatalog>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRuntimeCatalog {
  pub catalog: RuntimeCatalog,
  pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct CommunityWindowsCatalogInput<'a> {
  pub release_version: &'a str,
  pub catalog_sequence: u64,
  pub generated_at: &'a str,
  pub expires_at: &'a str,
  pub minimum_app_version: &'a str,
  pub package_manifest: &'a Path,
  pub package_directory: &'a Path,
  pub now_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub struct CommunityMacosCatalogInput<'a> {
  pub release_version: &'a str,
  pub catalog_sequence: u64,
  pub generated_at: &'a str,
  pub expires_at: &'a str,
  pub minimum_app_version: &'a str,
  pub package_manifest: &'a Path,
  pub package_directory: &'a Path,
  pub now_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub struct CommunityCatalogInput<'a> {
  pub release_version: &'a str,
  pub catalog_sequence: u64,
  pub generated_at: &'a str,
  pub expires_at: &'a str,
  pub minimum_app_version: &'a str,
  pub windows_package_manifest: &'a Path,
  pub windows_package_directory: &'a Path,
  pub macos_package_manifest: &'a Path,
  pub macos_package_directory: &'a Path,
  pub now_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub struct CommunityCatalogV2Input<'a> {
  pub catalog_sequence: u64,
  pub generated_at: &'a str,
  pub expires_at: &'a str,
  pub minimum_app_version: &'a str,
  pub runtime_index: &'a Path,
  pub now_unix_seconds: i64,
}

struct CommunityPackageCatalogInput<'a> {
  release_version: &'a str,
  catalog_sequence: u64,
  generated_at: &'a str,
  expires_at: &'a str,
  minimum_app_version: &'a str,
  package_manifest: &'a Path,
  package_directory: &'a Path,
  now_unix_seconds: i64,
  expected_platform: &'a str,
  expected_architecture: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallLayout {
  pub runtime_root: PathBuf,
  pub staging_root: PathBuf,
  pub active_link: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePackageReceipt {
  pub schema_version: u16,
  pub name: String,
  pub version: String,
  pub package_sha256: String,
  pub catalog_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeInstallTransaction {
  pub layout: InstallLayout,
  backup_root: Option<PathBuf>,
  active_before: Option<String>,
  removal_marker_before: Option<(PathBuf, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimePackageInstallInput<'a> {
  pub artifact: &'a Path,
  pub expected_sha256: &'a str,
  pub catalog_sequence: u64,
  pub name: &'a str,
  pub version: &'a str,
  pub base: &'a Path,
  pub activate: bool,
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
  #[error("runtime health check failed: {0}")]
  HealthCheckFailed(String),
  #[error("invalid runtime package receipt: {0}")]
  InvalidPackageReceipt(String),
  #[error("unable to parse runtime package receipt: {0}")]
  PackageReceiptJson(#[from] serde_json::Error),
  #[error("a runtime replacement backup already exists: {0}")]
  ReplacementBackupExists(PathBuf),
}

#[derive(Debug, Error)]
pub enum RuntimeCatalogError {
  #[error("runtime catalog exceeds the {RUNTIME_CATALOG_MAX_BYTES} byte limit")]
  TooLarge,
  #[error("unable to parse runtime catalog JSON: {0}")]
  Json(#[from] serde_json::Error),
  #[error("invalid runtime catalog {field}: {message}")]
  Invalid { field: String, message: String },
  #[error("fabDev {current} does not satisfy the runtime catalog minimum app version {minimum}")]
  IncompatibleApp { current: String, minimum: String },
  #[error(
    "Agent Protocol {current} does not satisfy the runtime catalog minimum protocol version {minimum}"
  )]
  IncompatibleProtocol { current: u16, minimum: u16 },
  #[error("runtime catalog sequence {received} is older than accepted sequence {accepted}")]
  SequenceRollback { received: u64, accepted: u64 },
  #[error("runtime catalog sequence {sequence} was reused with different contents")]
  SequenceHashMismatch { sequence: u64 },
}

#[derive(Debug, Error)]
pub enum RuntimeCatalogBuildError {
  #[error("unable to read Runtime package: {0}")]
  Read(#[from] std::io::Error),
  #[error("unable to parse Runtime package manifest: {0}")]
  PackageManifestJson(serde_json::Error),
  #[error("unable to parse Runtime Catalog index: {0}")]
  CatalogIndexJson(serde_json::Error),
  #[error("invalid Runtime package manifest {field}: {message}")]
  InvalidPackageManifest { field: String, message: String },
  #[error("invalid Runtime Catalog index {field}: {message}")]
  InvalidCatalogIndex { field: String, message: String },
  #[error(transparent)]
  Catalog(#[from] RuntimeCatalogError),
}

pub fn parse_and_validate_runtime_catalog(
  contents: &[u8],
  validation: &RuntimeCatalogValidation<'_>,
) -> Result<ValidatedRuntimeCatalog, RuntimeCatalogError> {
  if contents.len() > RUNTIME_CATALOG_MAX_BYTES {
    return Err(RuntimeCatalogError::TooLarge);
  }
  let document = serde_json::from_slice::<serde_json::Value>(contents)?;
  validate_signature_fields_present(&document)?;
  let catalog = serde_json::from_value::<RuntimeCatalog>(document)?;
  let sha256 = hex::encode(Sha256::digest(contents));
  validate_runtime_catalog(&catalog, &sha256, validation)?;
  Ok(ValidatedRuntimeCatalog { catalog, sha256 })
}

fn validate_signature_fields_present(
  document: &serde_json::Value,
) -> Result<(), RuntimeCatalogError> {
  let object = document
    .as_object()
    .ok_or_else(|| invalid_catalog("document", "must be a JSON object"))?;
  require_catalog_value(
    object.contains_key("signature"),
    "signature",
    "must be present and null",
  )?;
  let runtimes = object
    .get("runtimes")
    .and_then(serde_json::Value::as_array)
    .ok_or_else(|| invalid_catalog("runtimes", "must be an array"))?;
  for (index, runtime) in runtimes.iter().enumerate() {
    let has_signature = runtime
      .as_object()
      .is_some_and(|runtime| runtime.contains_key("signature"));
    require_catalog_value(
      has_signature,
      format!("runtimes[{index}].signature"),
      "must be present and null",
    )?;
  }
  Ok(())
}

pub fn generate_runtime_catalog(
  catalog: &RuntimeCatalog,
  validation: &RuntimeCatalogValidation<'_>,
) -> Result<Vec<u8>, RuntimeCatalogError> {
  let contents = serde_json::to_vec_pretty(catalog)?;
  if contents.len() > RUNTIME_CATALOG_MAX_BYTES {
    return Err(RuntimeCatalogError::TooLarge);
  }
  let sha256 = hex::encode(Sha256::digest(&contents));
  validate_runtime_catalog(catalog, &sha256, validation)?;
  Ok(contents)
}

pub fn generate_community_windows_catalog(
  input: &CommunityWindowsCatalogInput<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  generate_community_package_catalog(&CommunityPackageCatalogInput {
    release_version: input.release_version,
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at,
    expires_at: input.expires_at,
    minimum_app_version: input.minimum_app_version,
    package_manifest: input.package_manifest,
    package_directory: input.package_directory,
    now_unix_seconds: input.now_unix_seconds,
    expected_platform: "windows",
    expected_architecture: "x64",
  })
}

pub fn generate_community_catalog_v2(
  input: &CommunityCatalogV2Input<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  let contents = std::fs::read(input.runtime_index)?;
  let index = serde_json::from_slice::<RuntimeCatalogIndex>(&contents)
    .map_err(RuntimeCatalogBuildError::CatalogIndexJson)?;
  if index.schema_version != 1 {
    return Err(RuntimeCatalogBuildError::InvalidCatalogIndex {
      field: "schemaVersion".to_owned(),
      message: "must be 1".to_owned(),
    });
  }
  let catalog = RuntimeCatalog {
    schema_version: RUNTIME_CATALOG_SCHEMA_VERSION_V2,
    product: RUNTIME_CATALOG_PRODUCT.to_owned(),
    channel: RUNTIME_CATALOG_CHANNEL.to_owned(),
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at.to_owned(),
    expires_at: input.expires_at.to_owned(),
    unsigned_community_build: true,
    integrity: "sha256".to_owned(),
    compatibility: RuntimeCatalogCompatibility {
      minimum_app_version: input.minimum_app_version.to_owned(),
      minimum_agent_protocol_version: COMMUNITY_RUNTIME_CATALOG_V2_MINIMUM_PROTOCOL_VERSION,
    },
    signature: None,
    runtimes: index.runtimes,
  };
  let validation = RuntimeCatalogValidation {
    current_app_version: input.minimum_app_version,
    current_agent_protocol_version: COMMUNITY_RUNTIME_CATALOG_V2_MINIMUM_PROTOCOL_VERSION,
    now_unix_seconds: input.now_unix_seconds,
    accepted_catalog: None,
  };
  let contents = generate_runtime_catalog(&catalog, &validation)?;
  parse_and_validate_runtime_catalog(&contents, &validation)?;
  Ok(contents)
}

fn generate_community_package_catalog(
  input: &CommunityPackageCatalogInput<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  let manifest = read_runtime_package_manifest(input.package_manifest)?;
  validate_runtime_package_manifest(
    &manifest,
    input.expected_platform,
    input.expected_architecture,
  )?;
  let release_url = |file_name: &str| {
    format!(
      "{RELEASE_DOWNLOAD_PREFIX}{}/{file_name}",
      input.release_version
    )
  };
  let mut runtimes = Vec::with_capacity(manifest.packages.len());
  for package in &manifest.packages {
    let file_name = format!(
      "{}-{}-{}-{}-community.tar.gz",
      package.name, package.version, manifest.platform, manifest.architecture
    );
    let path = input.package_directory.join(&file_name);
    let (size, sha256) = file_size_and_sha256(&path)?;
    runtimes.push(RuntimeRelease {
      name: package.name.clone(),
      version: package.version.clone(),
      platform: manifest.platform.clone(),
      architecture: manifest.architecture.clone(),
      minimum_os_version: Some(
        package
          .minimum_os_version
          .clone()
          .unwrap_or_else(|| manifest.minimum_os_version.clone()),
      ),
      file_name: Some(file_name.clone()),
      url: release_url(&file_name),
      size,
      sha256,
      signature: None,
      source_verification: Some(RuntimeSourceVerification {
        method: package.source.verification.method.clone(),
        fingerprint: package.source.verification.fingerprint.clone(),
        upstream_sha256: package.source.archive_sha256.clone(),
      }),
      archive_format: Some("tar.gz".to_owned()),
      install_mode: Some("side-by-side".to_owned()),
      health_check_profile: Some(package.health_check_profile.clone()),
    });
  }
  let catalog = RuntimeCatalog {
    schema_version: RUNTIME_CATALOG_SCHEMA_VERSION,
    product: RUNTIME_CATALOG_PRODUCT.to_owned(),
    channel: RUNTIME_CATALOG_CHANNEL.to_owned(),
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at.to_owned(),
    expires_at: input.expires_at.to_owned(),
    unsigned_community_build: true,
    integrity: "sha256".to_owned(),
    compatibility: RuntimeCatalogCompatibility {
      minimum_app_version: input.minimum_app_version.to_owned(),
      minimum_agent_protocol_version: COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    },
    signature: None,
    runtimes,
  };
  let validation = RuntimeCatalogValidation {
    current_app_version: input.minimum_app_version,
    current_agent_protocol_version: COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    now_unix_seconds: input.now_unix_seconds,
    accepted_catalog: None,
  };
  let contents = generate_runtime_catalog(&catalog, &validation)?;
  parse_and_validate_runtime_catalog(&contents, &validation)?;
  Ok(contents)
}

pub fn read_runtime_package_manifest(
  path: &Path,
) -> Result<RuntimePackageManifest, RuntimeCatalogBuildError> {
  let contents = std::fs::read(path)?;
  serde_json::from_slice(&contents).map_err(RuntimeCatalogBuildError::PackageManifestJson)
}

fn validate_runtime_package_manifest(
  manifest: &RuntimePackageManifest,
  expected_platform: &str,
  expected_architecture: &str,
) -> Result<(), RuntimeCatalogBuildError> {
  require_package_manifest_value(manifest.schema_version == 1, "schemaVersion", "must be 1")?;
  require_package_manifest_value(
    manifest.platform == expected_platform,
    "platform",
    &format!("must be {expected_platform}"),
  )?;
  require_package_manifest_value(
    manifest.architecture == expected_architecture,
    "architecture",
    &format!("must be {expected_architecture}"),
  )?;
  validate_numeric_version(&manifest.minimum_os_version, "minimumOsVersion")
    .map_err(RuntimeCatalogBuildError::Catalog)?;
  require_package_manifest_value(
    !manifest.packages.is_empty(),
    "packages",
    "must not be empty",
  )?;

  let mut identities = HashSet::new();
  for (index, package) in manifest.packages.iter().enumerate() {
    let field = |name: &str| format!("packages[{index}].{name}");
    require_package_manifest_value(
      matches!(package.name.as_str(), "php" | "mariadb" | "node"),
      &field("name"),
      "must be php, mariadb, or node",
    )?;
    let version = Version::parse(&package.version).map_err(|error| {
      invalid_package_manifest(field("version"), format!("must be stable SemVer: {error}"))
    })?;
    require_package_manifest_value(
      version.pre.is_empty() && version.build.is_empty(),
      &field("version"),
      "must be stable SemVer without prerelease or build metadata",
    )?;
    require_package_manifest_value(
      identities.insert((package.name.as_str(), package.version.as_str())),
      &field("version"),
      "duplicates an existing package identity",
    )?;
    if let Some(minimum_os_version) = &package.minimum_os_version {
      validate_numeric_version(minimum_os_version, &field("minimumOsVersion"))
        .map_err(RuntimeCatalogBuildError::Catalog)?;
    }
    require_package_manifest_value(
      package.source.archive_url.starts_with("https://"),
      &field("source.archiveUrl"),
      "must use HTTPS",
    )?;
    require_lowercase_sha256(
      &package.source.archive_sha256,
      &field("source.archiveSha256"),
    )
    .map_err(RuntimeCatalogBuildError::Catalog)?;
    for (name, value) in [
      (
        "source.signatureUrl",
        package.source.signature_url.as_deref(),
      ),
      (
        "source.signedChecksumsUrl",
        package.source.signed_checksums_url.as_deref(),
      ),
      ("source.keyUrl", package.source.key_url.as_deref()),
    ] {
      if let Some(value) = value {
        require_package_manifest_value(
          value.starts_with("https://"),
          &field(name),
          "must use HTTPS",
        )?;
      }
    }
    validate_package_verification(package, index)?;
    let expected_profile = format!("{}-runtime-v1", package.name);
    require_package_manifest_value(
      package.health_check_profile == expected_profile,
      &field("healthCheckProfile"),
      &format!("must be {expected_profile}"),
    )?;
  }
  Ok(())
}

fn validate_package_verification(
  package: &RuntimePackageDefinition,
  index: usize,
) -> Result<(), RuntimeCatalogBuildError> {
  let field = |name: &str| format!("packages[{index}].source.verification.{name}");
  match package.source.verification.method.as_str() {
    "official-sha256" => require_package_manifest_value(
      package.source.verification.fingerprint.is_none(),
      &field("fingerprint"),
      "must be omitted for official-sha256",
    ),
    "pgp" => {
      let fingerprint = package
        .source
        .verification
        .fingerprint
        .as_deref()
        .ok_or_else(|| invalid_package_manifest(field("fingerprint"), "is required for pgp"))?;
      require_package_manifest_value(
        fingerprint.len() == 40
          && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)),
        &field("fingerprint"),
        "must be 40 uppercase hexadecimal characters",
      )?;
      require_package_manifest_value(
        package.source.key_url.is_some(),
        &format!("packages[{index}].source.keyUrl"),
        "is required for pgp",
      )?;
      require_package_manifest_value(
        package.source.signature_url.is_some() || package.source.signed_checksums_url.is_some(),
        &format!("packages[{index}].source"),
        "must provide signatureUrl or signedChecksumsUrl for pgp",
      )
    }
    _ => Err(invalid_package_manifest(
      field("method"),
      "must be official-sha256 or pgp",
    )),
  }
}

fn require_package_manifest_value(
  condition: bool,
  field: &str,
  message: &str,
) -> Result<(), RuntimeCatalogBuildError> {
  if condition {
    Ok(())
  } else {
    Err(invalid_package_manifest(field, message))
  }
}

fn invalid_package_manifest(
  field: impl Into<String>,
  message: impl Into<String>,
) -> RuntimeCatalogBuildError {
  RuntimeCatalogBuildError::InvalidPackageManifest {
    field: field.into(),
    message: message.into(),
  }
}

pub fn generate_community_macos_catalog(
  input: &CommunityMacosCatalogInput<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  generate_community_package_catalog(&CommunityPackageCatalogInput {
    release_version: input.release_version,
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at,
    expires_at: input.expires_at,
    minimum_app_version: input.minimum_app_version,
    package_manifest: input.package_manifest,
    package_directory: input.package_directory,
    now_unix_seconds: input.now_unix_seconds,
    expected_platform: "macos",
    expected_architecture: "arm64",
  })
}

pub fn generate_community_catalog(
  input: &CommunityCatalogInput<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  let validation = RuntimeCatalogValidation {
    current_app_version: input.minimum_app_version,
    current_agent_protocol_version: COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    now_unix_seconds: input.now_unix_seconds,
    accepted_catalog: None,
  };
  let windows_contents = generate_community_windows_catalog(&CommunityWindowsCatalogInput {
    release_version: input.release_version,
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at,
    expires_at: input.expires_at,
    minimum_app_version: input.minimum_app_version,
    package_manifest: input.windows_package_manifest,
    package_directory: input.windows_package_directory,
    now_unix_seconds: input.now_unix_seconds,
  })?;
  let macos_contents = generate_community_macos_catalog(&CommunityMacosCatalogInput {
    release_version: input.release_version,
    catalog_sequence: input.catalog_sequence,
    generated_at: input.generated_at,
    expires_at: input.expires_at,
    minimum_app_version: input.minimum_app_version,
    package_manifest: input.macos_package_manifest,
    package_directory: input.macos_package_directory,
    now_unix_seconds: input.now_unix_seconds,
  })?;
  let mut catalog = parse_and_validate_runtime_catalog(&windows_contents, &validation)?.catalog;
  let macos_catalog = parse_and_validate_runtime_catalog(&macos_contents, &validation)?.catalog;
  catalog.runtimes.extend(macos_catalog.runtimes);
  let contents = generate_runtime_catalog(&catalog, &validation)?;
  parse_and_validate_runtime_catalog(&contents, &validation)?;
  Ok(contents)
}

fn file_size_and_sha256(path: &Path) -> Result<(u64, String), std::io::Error> {
  let file = File::open(path)?;
  let mut reader = BufReader::new(file);
  let mut hasher = Sha256::new();
  let mut size = 0_u64;
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = reader.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    size += count as u64;
    hasher.update(&buffer[..count]);
  }
  Ok((size, hex::encode(hasher.finalize())))
}

pub fn validate_runtime_catalog(
  catalog: &RuntimeCatalog,
  catalog_sha256: &str,
  validation: &RuntimeCatalogValidation<'_>,
) -> Result<(), RuntimeCatalogError> {
  require_catalog_value(
    matches!(
      catalog.schema_version,
      RUNTIME_CATALOG_SCHEMA_VERSION | RUNTIME_CATALOG_SCHEMA_VERSION_V2
    ),
    "schemaVersion",
    format!("must be {RUNTIME_CATALOG_SCHEMA_VERSION} or {RUNTIME_CATALOG_SCHEMA_VERSION_V2}"),
  )?;
  require_catalog_value(
    catalog.product == RUNTIME_CATALOG_PRODUCT,
    "product",
    format!("must be {RUNTIME_CATALOG_PRODUCT}"),
  )?;
  require_catalog_value(
    catalog.channel == RUNTIME_CATALOG_CHANNEL,
    "channel",
    format!("must be {RUNTIME_CATALOG_CHANNEL}"),
  )?;
  require_catalog_value(
    catalog.catalog_sequence > 0 && catalog.catalog_sequence <= MAX_SAFE_JSON_INTEGER,
    "catalogSequence",
    format!("must be between 1 and {MAX_SAFE_JSON_INTEGER}"),
  )?;
  require_catalog_value(
    catalog.unsigned_community_build,
    "unsignedCommunityBuild",
    "must be true for Community v1",
  )?;
  require_catalog_value(catalog.integrity == "sha256", "integrity", "must be sha256")?;
  require_catalog_value(
    catalog.signature.is_none(),
    "signature",
    "must be null for Unsigned Community v1",
  )?;
  require_lowercase_sha256(catalog_sha256, "catalogSha256")?;

  let generated_at = parse_rfc3339_utc(&catalog.generated_at, "generatedAt")?;
  let expires_at = parse_rfc3339_utc(&catalog.expires_at, "expiresAt")?;
  require_catalog_value(
    generated_at <= validation.now_unix_seconds + MAX_GENERATED_AT_FUTURE_SECONDS,
    "generatedAt",
    "must not be more than five minutes in the future",
  )?;
  require_catalog_value(
    expires_at > generated_at,
    "expiresAt",
    "must be later than generatedAt",
  )?;
  require_catalog_value(
    expires_at > validation.now_unix_seconds,
    "expiresAt",
    "catalog is expired",
  )?;

  let minimum_app_version = Version::parse(&catalog.compatibility.minimum_app_version)
    .map_err(|error| invalid_catalog("compatibility.minimumAppVersion", error.to_string()))?;
  let current_app_version = Version::parse(validation.current_app_version)
    .map_err(|error| invalid_catalog("currentAppVersion", error.to_string()))?;
  if current_app_version < minimum_app_version {
    return Err(RuntimeCatalogError::IncompatibleApp {
      current: current_app_version.to_string(),
      minimum: minimum_app_version.to_string(),
    });
  }
  require_catalog_value(
    catalog.compatibility.minimum_agent_protocol_version
      >= RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    "compatibility.minimumAgentProtocolVersion",
    format!("must be at least {RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION} for schema v1"),
  )?;
  if validation.current_agent_protocol_version
    < catalog.compatibility.minimum_agent_protocol_version
  {
    return Err(RuntimeCatalogError::IncompatibleProtocol {
      current: validation.current_agent_protocol_version,
      minimum: catalog.compatibility.minimum_agent_protocol_version,
    });
  }

  if let Some(accepted) = validation.accepted_catalog {
    require_catalog_value(
      accepted.schema_version == catalog.schema_version,
      "acceptedCatalog.schemaVersion",
      "must match the Runtime Catalog schemaVersion",
    )?;
    require_lowercase_sha256(&accepted.sha256, "acceptedCatalog.sha256")?;
    if catalog.catalog_sequence < accepted.sequence {
      return Err(RuntimeCatalogError::SequenceRollback {
        received: catalog.catalog_sequence,
        accepted: accepted.sequence,
      });
    }
    if catalog.catalog_sequence == accepted.sequence && catalog_sha256 != accepted.sha256 {
      return Err(RuntimeCatalogError::SequenceHashMismatch {
        sequence: catalog.catalog_sequence,
      });
    }
  }

  require_catalog_value(
    !catalog.runtimes.is_empty(),
    "runtimes",
    "must not be empty",
  )?;
  let mut identities = HashSet::new();
  for (index, release) in catalog.runtimes.iter().enumerate() {
    validate_catalog_release(
      release,
      index,
      catalog.schema_version,
      catalog.catalog_sequence,
    )?;
    let identity = (
      release.name.as_str(),
      release.version.as_str(),
      release.platform.as_str(),
      release.architecture.as_str(),
    );
    require_catalog_value(
      identities.insert(identity),
      format!("runtimes[{index}]"),
      "duplicates an existing Runtime identity",
    )?;
  }
  Ok(())
}

fn validate_catalog_release(
  release: &RuntimeRelease,
  index: usize,
  schema_version: u16,
  catalog_sequence: u64,
) -> Result<(), RuntimeCatalogError> {
  let field = |name: &str| format!("runtimes[{index}].{name}");
  require_catalog_value(
    matches!(release.name.as_str(), "php" | "mariadb" | "node"),
    field("name"),
    "must be php, mariadb, or node",
  )?;
  let supported_target = matches!(
    (
      release.name.as_str(),
      release.platform.as_str(),
      release.architecture.as_str()
    ),
    ("php", "macos", "arm64")
      | ("php", "windows", "x64")
      | ("mariadb", "macos", "arm64")
      | ("mariadb", "windows", "x64")
      | ("node", "macos", "arm64")
      | ("node", "windows", "x64")
  );
  require_catalog_value(
    supported_target,
    field("platform"),
    "must target a supported Runtime platform and architecture",
  )?;
  validate_catalog_runtime_version(release, &field("version"))?;
  let minimum_os_version = release
    .minimum_os_version
    .as_deref()
    .ok_or_else(|| invalid_catalog(field("minimumOsVersion"), "is required"))?;
  validate_numeric_version(minimum_os_version, &field("minimumOsVersion"))?;

  let expected_file_name = format!(
    "{}-{}-{}-{}-community.tar.gz",
    release.name, release.version, release.platform, release.architecture
  );
  let file_name = release
    .file_name
    .as_deref()
    .ok_or_else(|| invalid_catalog(field("fileName"), "is required"))?;
  require_catalog_value(
    file_name == expected_file_name,
    field("fileName"),
    format!("must be {expected_file_name}"),
  )?;
  match schema_version {
    RUNTIME_CATALOG_SCHEMA_VERSION => {
      validate_versioned_release_url(&release.url, file_name, &field("url"))?;
    }
    RUNTIME_CATALOG_SCHEMA_VERSION_V2 => {
      let package_sequence = validate_runtime_release_url(&release.url, file_name, &field("url"))?;
      require_catalog_value(
        package_sequence <= catalog_sequence,
        field("url"),
        "must not reference a future Runtime Catalog Release",
      )?;
    }
    _ => unreachable!("Runtime Catalog schema was validated"),
  }
  require_catalog_value(
    release.size > 0 && release.size <= MAX_RUNTIME_BYTES,
    field("size"),
    format!("must be between 1 and {MAX_RUNTIME_BYTES} bytes"),
  )?;
  require_lowercase_sha256(&release.sha256, &field("sha256"))?;
  require_catalog_value(
    release.signature.is_none(),
    field("signature"),
    "must be null for Unsigned Community v1",
  )?;

  let source = release
    .source_verification
    .as_ref()
    .ok_or_else(|| invalid_catalog(field("sourceVerification"), "is required"))?;
  require_lowercase_sha256(
    &source.upstream_sha256,
    &field("sourceVerification.upstreamSha256"),
  )?;
  validate_catalog_source_verification(release, source, &field)?;
  require_optional_catalog_value(
    release.archive_format.as_deref(),
    "tar.gz",
    field("archiveFormat"),
  )?;
  require_optional_catalog_value(
    release.install_mode.as_deref(),
    "side-by-side",
    field("installMode"),
  )?;
  require_optional_catalog_value(
    release.health_check_profile.as_deref(),
    match release.name.as_str() {
      "php" => "php-runtime-v1",
      "mariadb" => "mariadb-runtime-v1",
      "node" => "node-runtime-v1",
      _ => unreachable!("Runtime name was validated"),
    },
    field("healthCheckProfile"),
  )?;
  Ok(())
}

fn validate_catalog_runtime_version(
  release: &RuntimeRelease,
  field: &str,
) -> Result<(), RuntimeCatalogError> {
  let version = Version::parse(&release.version)
    .map_err(|error| invalid_catalog(field, format!("must be stable SemVer: {error}")))?;
  let minimum_major = match release.name.as_str() {
    "php" => 7,
    "mariadb" => 10,
    "node" => 20,
    _ => unreachable!("Runtime name was validated"),
  };
  require_catalog_value(
    version.major >= minimum_major && version.pre.is_empty() && version.build.is_empty(),
    field,
    format!("must be stable SemVer {minimum_major}.0.0 or newer"),
  )
}

fn validate_catalog_source_verification(
  _release: &RuntimeRelease,
  source: &RuntimeSourceVerification,
  field: &impl Fn(&str) -> String,
) -> Result<(), RuntimeCatalogError> {
  match source.method.as_str() {
    "official-sha256" => require_catalog_value(
      source.fingerprint.is_none(),
      field("sourceVerification.fingerprint"),
      "must be omitted for official-sha256",
    ),
    "pgp" => {
      let fingerprint = source.fingerprint.as_deref().ok_or_else(|| {
        invalid_catalog(
          field("sourceVerification.fingerprint"),
          "is required for pgp",
        )
      })?;
      require_catalog_value(
        fingerprint.len() == 40
          && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)),
        field("sourceVerification.fingerprint"),
        "must be 40 uppercase hexadecimal characters",
      )
    }
    _ => Err(invalid_catalog(
      field("sourceVerification.method"),
      "must be official-sha256 or pgp",
    )),
  }
}

fn validate_versioned_release_url(
  url: &str,
  file_name: &str,
  field: &str,
) -> Result<(), RuntimeCatalogError> {
  let remainder = url
    .strip_prefix(RELEASE_DOWNLOAD_PREFIX)
    .ok_or_else(|| invalid_catalog(field, "must use the official versioned GitHub Release URL"))?;
  let suffix = format!("/{file_name}");
  let release_version = remainder
    .strip_suffix(&suffix)
    .filter(|version| !version.is_empty() && !version.contains('/'))
    .ok_or_else(|| invalid_catalog(field, "must end with the declared fileName"))?;
  Version::parse(release_version)
    .map_err(|error| invalid_catalog(field, format!("contains an invalid release tag: {error}")))?;
  Ok(())
}

fn validate_runtime_release_url(
  url: &str,
  file_name: &str,
  field: &str,
) -> Result<u64, RuntimeCatalogError> {
  let remainder = url
    .strip_prefix(RUNTIME_RELEASE_DOWNLOAD_PREFIX)
    .ok_or_else(|| {
      invalid_catalog(
        field,
        "must use the official versioned fabdev-runtimes Release URL",
      )
    })?;
  let suffix = format!("/{file_name}");
  let catalog_sequence = remainder
    .strip_suffix(&suffix)
    .filter(|sequence| !sequence.is_empty() && !sequence.contains('/'))
    .ok_or_else(|| invalid_catalog(field, "must end with the declared fileName"))?;
  let catalog_sequence = catalog_sequence
    .parse::<u64>()
    .map_err(|error| invalid_catalog(field, format!("contains an invalid Catalog tag: {error}")))?;
  require_catalog_value(
    catalog_sequence > 0 && catalog_sequence <= MAX_SAFE_JSON_INTEGER,
    field,
    format!("Catalog tag must be between 1 and {MAX_SAFE_JSON_INTEGER}"),
  )?;
  Ok(catalog_sequence)
}

fn require_optional_catalog_value(
  actual: Option<&str>,
  expected: &str,
  field: String,
) -> Result<(), RuntimeCatalogError> {
  require_catalog_value(
    actual == Some(expected),
    field,
    format!("must be {expected}"),
  )
}

fn require_lowercase_sha256(value: &str, field: &str) -> Result<(), RuntimeCatalogError> {
  require_catalog_value(
    value.len() == 64
      && value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
    field,
    "must be 64 lowercase hexadecimal characters",
  )
}

fn validate_numeric_version(value: &str, field: &str) -> Result<(), RuntimeCatalogError> {
  let parts = value.split('.').collect::<Vec<_>>();
  require_catalog_value(
    matches!(parts.len(), 2 | 3)
      && parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
    field,
    "must contain two or three numeric components",
  )
}

fn parse_rfc3339_utc(value: &str, field: &str) -> Result<i64, RuntimeCatalogError> {
  if value.len() < 20 || !value.ends_with('Z') || value.as_bytes().get(10) != Some(&b'T') {
    return Err(invalid_catalog(field, "must be an RFC 3339 UTC timestamp"));
  }
  let date = &value[..10];
  let time = &value[11..value.len() - 1];
  let date_parts = date.split('-').collect::<Vec<_>>();
  let (clock, fraction) = time
    .split_once('.')
    .map_or((time, None), |(clock, fraction)| (clock, Some(fraction)));
  let time_parts = clock.split(':').collect::<Vec<_>>();
  if date_parts.len() != 3
    || date_parts[0].len() != 4
    || date_parts[1].len() != 2
    || date_parts[2].len() != 2
    || time_parts.len() != 3
    || time_parts.iter().any(|part| part.len() != 2)
    || fraction
      .is_some_and(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
  {
    return Err(invalid_catalog(field, "must be an RFC 3339 UTC timestamp"));
  }
  let parse = |part: &str| {
    part
      .parse::<u32>()
      .map_err(|_| invalid_catalog(field, "must be an RFC 3339 UTC timestamp"))
  };
  let year = parse(date_parts[0])? as i32;
  let month = parse(date_parts[1])?;
  let day = parse(date_parts[2])?;
  let hour = parse(time_parts[0])?;
  let minute = parse(time_parts[1])?;
  let second = parse(time_parts[2])?;
  let days_in_month = match month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
    4 | 6 | 9 | 11 => 30,
    2 if is_leap_year(year) => 29,
    2 => 28,
    _ => 0,
  };
  if year < 1970 || day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
    return Err(invalid_catalog(
      field,
      "contains an invalid UTC date or time",
    ));
  }
  let days = days_from_civil(year, month, day);
  Ok(days * 86_400 + i64::from(hour * 3_600 + minute * 60 + second))
}

fn is_leap_year(year: i32) -> bool {
  year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
  let adjusted_year = year - i32::from(month <= 2);
  let era = if adjusted_year >= 0 {
    adjusted_year
  } else {
    adjusted_year - 399
  } / 400;
  let year_of_era = adjusted_year - era * 400;
  let adjusted_month = month as i32 + if month > 2 { -3 } else { 9 };
  let day_of_year = (153 * adjusted_month + 2) / 5 + day as i32 - 1;
  let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
  i64::from(era * 146_097 + day_of_era - 719_468)
}

fn require_catalog_value(
  valid: bool,
  field: impl Into<String>,
  message: impl Into<String>,
) -> Result<(), RuntimeCatalogError> {
  if valid {
    Ok(())
  } else {
    Err(invalid_catalog(field, message))
  }
}

fn invalid_catalog(field: impl Into<String>, message: impl Into<String>) -> RuntimeCatalogError {
  RuntimeCatalogError::Invalid {
    field: field.into(),
    message: message.into(),
  }
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
  install_tar_gz_with_health_check(
    artifact,
    expected_sha256,
    name,
    version,
    base,
    activate,
    |_| Ok(()),
  )
}

pub fn install_tar_gz_with_health_check<F>(
  artifact: impl AsRef<Path>,
  expected_sha256: &str,
  name: &str,
  version: &str,
  base: impl AsRef<Path>,
  activate: bool,
  health_check: F,
) -> Result<InstallLayout, RuntimeError>
where
  F: FnOnce(&Path) -> Result<(), RuntimeError>,
{
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

  let staged_result = (|| {
    let archive = File::open(artifact)?;
    let mut archive = tar::Archive::new(GzDecoder::new(BufReader::new(archive)));
    #[cfg(windows)]
    // Windows can reject directory timestamp writes after all payload files were extracted.
    archive.set_preserve_mtime(false);
    archive.unpack(&layout.staging_root)?;
    let extracted = layout.staging_root.join(version);
    if !extracted.is_dir() {
      return Err(RuntimeError::InvalidArchive(version.to_owned()));
    }
    health_check(&extracted)?;
    Ok(extracted)
  })();
  let extracted = match staged_result {
    Ok(extracted) => extracted,
    Err(error) => {
      remove_dir_if_exists(&layout.staging_root)?;
      return Err(error);
    }
  };

  let runtime_parent = layout
    .runtime_root
    .parent()
    .ok_or_else(|| RuntimeError::InvalidArchive(name.to_owned()))?
    .to_path_buf();
  std::fs::create_dir_all(&runtime_parent)?;
  rename_runtime_directory(&extracted, &layout.runtime_root)?;
  remove_dir_if_exists(&layout.staging_root)?;
  if activate {
    switch_current(&runtime_parent, version, &layout.active_link)?;
  }
  clear_runtime_removal_marker(base, name, version)?;
  Ok(layout)
}

pub fn install_or_replace_tar_gz_with_health_check<F>(
  input: RuntimePackageInstallInput<'_>,
  health_check: F,
) -> Result<RuntimeInstallTransaction, RuntimeError>
where
  F: FnOnce(&Path) -> Result<(), RuntimeError>,
{
  let RuntimePackageInstallInput {
    artifact,
    expected_sha256,
    catalog_sequence,
    name,
    version,
    base,
    activate,
  } = input;
  validate_identifier(name)?;
  validate_identifier(version)?;
  validate_package_sha256(expected_sha256)?;
  if catalog_sequence == 0 || catalog_sequence > MAX_SAFE_JSON_INTEGER {
    return Err(RuntimeError::InvalidPackageReceipt(
      "catalogSequence is outside the safe integer range".to_owned(),
    ));
  }
  verify_sha256(artifact, expected_sha256)?;
  let layout = InstallLayout::new(base, name, version);
  let active_before = active_version(base, name)?;
  let removal_marker = runtime_removal_marker(base, name, version)?;
  let removal_marker_before = match std::fs::read(&removal_marker) {
    Ok(contents) => Some((removal_marker, contents)),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
    Err(error) => return Err(error.into()),
  };
  let runtime_parent = layout
    .runtime_root
    .parent()
    .ok_or_else(|| RuntimeError::InvalidArchive(name.to_owned()))?
    .to_path_buf();
  let backup_root = runtime_parent.join(format!(".{version}.fabdev-backup"));
  if backup_root.exists() {
    return Err(RuntimeError::ReplacementBackupExists(backup_root));
  }

  let replacing = layout.runtime_root.exists();
  if replacing
    && installed_runtime_package_sha256(base, name, version)?.as_deref() == Some(expected_sha256)
  {
    return Err(RuntimeError::AlreadyInstalled(layout.runtime_root));
  }
  if replacing {
    let metadata = std::fs::symlink_metadata(&layout.runtime_root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
      return Err(RuntimeError::AlreadyInstalled(layout.runtime_root));
    }
  }

  remove_dir_if_exists(&layout.staging_root)?;
  std::fs::create_dir_all(&layout.staging_root)?;
  let staged_result = (|| {
    let archive = File::open(artifact)?;
    let mut archive = tar::Archive::new(GzDecoder::new(BufReader::new(archive)));
    #[cfg(windows)]
    // Windows can reject directory timestamp writes after all payload files were extracted.
    archive.set_preserve_mtime(false);
    archive.unpack(&layout.staging_root)?;
    let extracted = layout.staging_root.join(version);
    if !extracted.is_dir() {
      return Err(RuntimeError::InvalidArchive(version.to_owned()));
    }
    write_runtime_package_receipt_at(
      &extracted,
      &RuntimePackageReceipt {
        schema_version: 1,
        name: name.to_owned(),
        version: version.to_owned(),
        package_sha256: expected_sha256.to_owned(),
        catalog_sequence,
      },
    )?;
    health_check(&extracted)?;
    Ok(extracted)
  })();
  let extracted = match staged_result {
    Ok(extracted) => extracted,
    Err(error) => {
      remove_dir_if_exists(&layout.staging_root)?;
      return Err(error);
    }
  };

  std::fs::create_dir_all(&runtime_parent)?;
  if replacing {
    rename_runtime_directory(&layout.runtime_root, &backup_root)?;
  }
  if let Err(error) = rename_runtime_directory(&extracted, &layout.runtime_root) {
    if replacing {
      let _ = rename_runtime_directory(&backup_root, &layout.runtime_root);
    }
    remove_dir_if_exists(&layout.staging_root)?;
    return Err(error.into());
  }
  let transaction = RuntimeInstallTransaction {
    layout,
    backup_root: replacing.then_some(backup_root),
    active_before,
    removal_marker_before,
  };
  let finish_result = (|| {
    remove_dir_if_exists(&transaction.layout.staging_root)?;
    if activate {
      switch_current(&runtime_parent, version, &transaction.layout.active_link)?;
    }
    clear_runtime_removal_marker(base, name, version)
  })();
  if let Err(error) = finish_result {
    rollback_runtime_install_transaction(transaction)?;
    return Err(error);
  }
  Ok(transaction)
}

pub fn commit_runtime_install_transaction(
  transaction: RuntimeInstallTransaction,
) -> Result<InstallLayout, RuntimeError> {
  if let Some(backup_root) = transaction.backup_root {
    remove_dir_if_exists(&backup_root)?;
  }
  Ok(transaction.layout)
}

pub fn rollback_runtime_install_transaction(
  transaction: RuntimeInstallTransaction,
) -> Result<(), RuntimeError> {
  remove_dir_if_exists(&transaction.layout.runtime_root)?;
  if let Some(backup_root) = transaction.backup_root {
    rename_runtime_directory(&backup_root, &transaction.layout.runtime_root)?;
  }
  remove_dir_if_exists(&transaction.layout.staging_root)?;
  let runtime_parent = transaction
    .layout
    .runtime_root
    .parent()
    .ok_or_else(|| RuntimeError::InvalidArchive("Runtime parent".to_owned()))?;
  match transaction.active_before {
    Some(version) => switch_current(runtime_parent, &version, &transaction.layout.active_link),
    None => remove_active_link_if_exists(&transaction.layout.active_link),
  }?;
  if let Some((path, contents)) = transaction.removal_marker_before {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
  }
  Ok(())
}

fn remove_active_link_if_exists(active_link: &Path) -> Result<(), RuntimeError> {
  match std::fs::remove_file(active_link) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

pub fn installed_runtime_package_sha256(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
) -> Result<Option<String>, RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  let runtime_root = base.as_ref().join(name).join(version);
  let receipt_path = runtime_root.join(RUNTIME_PACKAGE_RECEIPT_FILE);
  let contents = match std::fs::read(&receipt_path) {
    Ok(contents) => contents,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error.into()),
  };
  if contents.len() > 4096 {
    return Err(RuntimeError::InvalidPackageReceipt(
      "file exceeds 4096 bytes".to_owned(),
    ));
  }
  let receipt = serde_json::from_slice::<RuntimePackageReceipt>(&contents)?;
  validate_runtime_package_receipt(&receipt, name, version)?;
  Ok(Some(receipt.package_sha256))
}

pub fn record_runtime_package_receipt(
  base: impl AsRef<Path>,
  name: &str,
  version: &str,
  package_sha256: &str,
  catalog_sequence: u64,
) -> Result<(), RuntimeError> {
  validate_identifier(name)?;
  validate_identifier(version)?;
  validate_package_sha256(package_sha256)?;
  if catalog_sequence == 0 || catalog_sequence > MAX_SAFE_JSON_INTEGER {
    return Err(RuntimeError::InvalidPackageReceipt(
      "catalogSequence is outside the safe integer range".to_owned(),
    ));
  }
  let runtime_root = base.as_ref().join(name).join(version);
  if !runtime_root.is_dir() {
    return Err(RuntimeError::NotInstalled(
      name.to_owned(),
      version.to_owned(),
    ));
  }
  write_runtime_package_receipt_at(
    &runtime_root,
    &RuntimePackageReceipt {
      schema_version: 1,
      name: name.to_owned(),
      version: version.to_owned(),
      package_sha256: package_sha256.to_owned(),
      catalog_sequence,
    },
  )
}

fn write_runtime_package_receipt_at(
  runtime_root: &Path,
  receipt: &RuntimePackageReceipt,
) -> Result<(), RuntimeError> {
  let path = runtime_root.join(RUNTIME_PACKAGE_RECEIPT_FILE);
  let pending = runtime_root.join(format!("{RUNTIME_PACKAGE_RECEIPT_FILE}.pending"));
  let mut contents = serde_json::to_vec_pretty(receipt)?;
  contents.push(b'\n');
  std::fs::write(&pending, contents)?;
  match std::fs::remove_file(&path) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  std::fs::rename(pending, path)?;
  Ok(())
}

fn validate_runtime_package_receipt(
  receipt: &RuntimePackageReceipt,
  name: &str,
  version: &str,
) -> Result<(), RuntimeError> {
  if receipt.schema_version != 1 || receipt.name != name || receipt.version != version {
    return Err(RuntimeError::InvalidPackageReceipt(
      "identity does not match the installed Runtime".to_owned(),
    ));
  }
  validate_package_sha256(&receipt.package_sha256)?;
  if receipt.catalog_sequence == 0 || receipt.catalog_sequence > MAX_SAFE_JSON_INTEGER {
    return Err(RuntimeError::InvalidPackageReceipt(
      "catalogSequence is outside the safe integer range".to_owned(),
    ));
  }
  Ok(())
}

fn validate_package_sha256(value: &str) -> Result<(), RuntimeError> {
  if value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    Ok(())
  } else {
    Err(RuntimeError::InvalidPackageReceipt(
      "packageSha256 must be 64 lowercase hexadecimal characters".to_owned(),
    ))
  }
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
  let installed = match std::fs::symlink_metadata(parent.join(version)) {
    Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
    Err(error) => return Err(error.into()),
  };
  if !installed {
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
  match remove_runtime_directory(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn rename_runtime_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
  retry_windows_permission_denied(|| std::fs::rename(source, destination))
}

fn remove_runtime_directory(path: &Path) -> std::io::Result<()> {
  retry_windows_permission_denied(|| std::fs::remove_dir_all(path))
}

fn retry_windows_permission_denied<T>(
  operation: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
  #[cfg(windows)]
  {
    retry_permission_denied(51, Duration::from_millis(100), operation)
  }
  #[cfg(not(windows))]
  {
    retry_permission_denied(1, Duration::ZERO, operation)
  }
}

fn retry_permission_denied<T>(
  attempts: usize,
  delay: Duration,
  mut operation: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
  debug_assert!(attempts > 0);
  for attempt in 0..attempts {
    match operation() {
      Ok(value) => return Ok(value),
      Err(error)
        if error.kind() == std::io::ErrorKind::PermissionDenied && attempt + 1 < attempts =>
      {
        std::thread::sleep(delay);
      }
      Err(error) => return Err(error),
    }
  }
  unreachable!("the final retry attempt always returns")
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

  fn windows_package_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../resources/runtime-packages/windows-x64.json")
  }

  fn macos_package_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../resources/runtime-packages/macos-arm64.json")
  }

  fn create_manifest_packages(
    manifest_path: &Path,
    package_directory: &Path,
  ) -> RuntimePackageManifest {
    let manifest = read_runtime_package_manifest(manifest_path).expect("read package manifest");
    std::fs::create_dir_all(package_directory).expect("create package directory");
    for package in &manifest.packages {
      let file_name = format!(
        "{}-{}-{}-{}-community.tar.gz",
        package.name, package.version, manifest.platform, manifest.architecture
      );
      std::fs::write(
        package_directory.join(file_name),
        format!("{} {} runtime", package.name, package.version),
      )
      .expect("write Runtime package");
    }
    manifest
  }

  fn valid_catalog() -> RuntimeCatalog {
    RuntimeCatalog {
      schema_version: RUNTIME_CATALOG_SCHEMA_VERSION,
      product: RUNTIME_CATALOG_PRODUCT.to_owned(),
      channel: RUNTIME_CATALOG_CHANNEL.to_owned(),
      catalog_sequence: 7,
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
        size: 100,
        sha256: "a".repeat(64),
        signature: None,
        source_verification: Some(RuntimeSourceVerification {
          method: "pgp".to_owned(),
          fingerprint: Some("9D7F99A0CB8F05C8A6958D6256A97AF7600A39A6".to_owned()),
          upstream_sha256: "b".repeat(64),
        }),
        archive_format: Some("tar.gz".to_owned()),
        install_mode: Some("side-by-side".to_owned()),
        health_check_profile: Some("php-runtime-v1".to_owned()),
      }],
    }
  }

  fn catalog_validation<'a>(
    accepted_catalog: Option<&'a AcceptedRuntimeCatalog>,
  ) -> RuntimeCatalogValidation<'a> {
    RuntimeCatalogValidation {
      current_app_version: "0.1.4",
      current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
      now_unix_seconds: parse_rfc3339_utc("2026-08-30T00:01:00Z", "test").expect("parse test time"),
      accepted_catalog,
    }
  }

  #[test]
  fn generates_the_complete_windows_community_catalog() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-windows-complete-{}",
      uuid::Uuid::new_v4()
    ));
    let package_directory = root.join("packages");
    let package_manifest = windows_package_manifest_path();
    let manifest = create_manifest_packages(&package_manifest, &package_directory);

    let contents = generate_community_windows_catalog(&CommunityWindowsCatalogInput {
      release_version: "0.1.9",
      catalog_sequence: 3,
      generated_at: "2026-08-30T00:00:00Z",
      expires_at: "2027-02-26T00:00:00Z",
      minimum_app_version: "0.1.9",
      package_manifest: &package_manifest,
      package_directory: &package_directory,
      now_unix_seconds: parse_rfc3339_utc("2026-08-30T00:01:00Z", "test").expect("parse test time"),
    })
    .expect("generate complete Windows Community Catalog");
    let catalog: RuntimeCatalog = serde_json::from_slice(&contents).expect("parse Catalog");

    assert_eq!(catalog.catalog_sequence, 3);
    assert_eq!(
      catalog.compatibility.minimum_agent_protocol_version,
      COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION
    );
    assert_eq!(catalog.runtimes.len(), manifest.packages.len());
    assert_eq!(
      catalog
        .runtimes
        .iter()
        .map(|runtime| (&runtime.name, &runtime.version))
        .collect::<Vec<_>>(),
      manifest
        .packages
        .iter()
        .map(|package| (&package.name, &package.version))
        .collect::<Vec<_>>()
    );
    for (runtime, package) in catalog.runtimes.iter().zip(&manifest.packages) {
      assert_eq!(
        runtime.health_check_profile.as_deref(),
        Some(package.health_check_profile.as_str())
      );
      assert_eq!(
        runtime
          .source_verification
          .as_ref()
          .and_then(|source| source.fingerprint.as_deref()),
        package.source.verification.fingerprint.as_deref()
      );
    }
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn accepts_future_windows_package_versions_from_the_manifest() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-windows-future-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog fixture");
    let package_manifest = root.join("windows-x64.json");
    let package_directory = root.join("packages");
    let manifest = RuntimePackageManifest {
      schema_version: 1,
      platform: "windows".to_owned(),
      architecture: "x64".to_owned(),
      minimum_os_version: "11.0".to_owned(),
      packages: vec![
        RuntimePackageDefinition {
          name: "php".to_owned(),
          version: "8.5.1".to_owned(),
          minimum_os_version: None,
          build_profile: None,
          default: None,
          source: RuntimePackageSource {
            archive_url: "https://windows.php.net/php-8.5.1.zip".to_owned(),
            archive_sha256: "a".repeat(64),
            signature_url: None,
            signed_checksums_url: None,
            key_url: None,
            verification: RuntimePackageVerification {
              method: "official-sha256".to_owned(),
              fingerprint: None,
            },
          },
          health_check_profile: "php-runtime-v1".to_owned(),
        },
        RuntimePackageDefinition {
          name: "node".to_owned(),
          version: "26.1.3".to_owned(),
          minimum_os_version: None,
          build_profile: None,
          default: None,
          source: RuntimePackageSource {
            archive_url: "https://nodejs.org/dist/v26.1.3/node-v26.1.3-win-x64.zip".to_owned(),
            archive_sha256: "b".repeat(64),
            signature_url: None,
            signed_checksums_url: Some(
              "https://nodejs.org/dist/v26.1.3/SHASUMS256.txt.asc".to_owned(),
            ),
            key_url: Some("https://nodejs.org/keys/release.asc".to_owned()),
            verification: RuntimePackageVerification {
              method: "pgp".to_owned(),
              fingerprint: Some("1234567890ABCDEF1234567890ABCDEF12345678".to_owned()),
            },
          },
          health_check_profile: "node-runtime-v1".to_owned(),
        },
      ],
    };
    std::fs::write(
      &package_manifest,
      serde_json::to_vec_pretty(&manifest).expect("serialize package manifest"),
    )
    .expect("write package manifest");
    create_manifest_packages(&package_manifest, &package_directory);

    let contents = generate_community_windows_catalog(&CommunityWindowsCatalogInput {
      release_version: "0.1.18",
      catalog_sequence: 4,
      generated_at: "2026-09-01T00:00:00Z",
      expires_at: "2027-03-01T00:00:00Z",
      minimum_app_version: "0.1.18",
      package_manifest: &package_manifest,
      package_directory: &package_directory,
      now_unix_seconds: parse_rfc3339_utc("2026-09-01T00:01:00Z", "test").expect("parse test time"),
    })
    .expect("generate future Windows Community Catalog");
    let catalog: RuntimeCatalog = serde_json::from_slice(&contents).expect("parse Catalog");

    assert_eq!(
      catalog
        .runtimes
        .iter()
        .map(|runtime| (runtime.name.as_str(), runtime.version.as_str()))
        .collect::<Vec<_>>(),
      vec![("php", "8.5.1"), ("node", "26.1.3")]
    );
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn generates_the_complete_macos_community_catalog() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-macos-complete-{}",
      uuid::Uuid::new_v4()
    ));
    let package_manifest = macos_package_manifest_path();
    let package_directory = root.join("packages");
    let manifest = create_manifest_packages(&package_manifest, &package_directory);

    let contents = generate_community_macos_catalog(&CommunityMacosCatalogInput {
      release_version: "0.1.12",
      catalog_sequence: 4,
      generated_at: "2026-08-31T00:00:00Z",
      expires_at: "2027-02-27T00:00:00Z",
      minimum_app_version: "0.1.12",
      package_manifest: &package_manifest,
      package_directory: &package_directory,
      now_unix_seconds: parse_rfc3339_utc("2026-08-31T00:01:00Z", "test").expect("parse test time"),
    })
    .expect("generate complete macOS Community Catalog");
    let catalog: RuntimeCatalog = serde_json::from_slice(&contents).expect("parse Catalog");

    assert_eq!(catalog.catalog_sequence, 4);
    assert_eq!(
      catalog.compatibility.minimum_agent_protocol_version,
      COMMUNITY_RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION
    );
    assert_eq!(catalog.runtimes.len(), manifest.packages.len());
    assert!(catalog
      .runtimes
      .iter()
      .all(|runtime| runtime.platform == "macos" && runtime.architecture == "arm64"));
    assert_eq!(catalog.runtimes[0].name, "php");
    assert_eq!(catalog.runtimes[1].name, "mariadb");
    assert_eq!(catalog.runtimes[2].version, "20.20.2");
    assert_eq!(catalog.runtimes[3].version, "24.20.0");
    assert_eq!(
      catalog.runtimes[3].minimum_os_version.as_deref(),
      Some("13.5")
    );
    assert_eq!(
      catalog.runtimes[1].file_name.as_deref(),
      Some("mariadb-12.3.2-macos-arm64-community.tar.gz")
    );
    assert_eq!(
      catalog.runtimes[2]
        .source_verification
        .as_ref()
        .map(|source| source.upstream_sha256.as_str()),
      Some("466e05f3477c20dfb723054dfebffe55bc74660ee77f612166fca121dacb65b6")
    );
    assert_eq!(
      catalog.runtimes[3]
        .source_verification
        .as_ref()
        .map(|source| source.upstream_sha256.as_str()),
      Some("40e5607e5ecb3db9192723776da2d75d966260fc74a7a9e731c1bd67dda96bc8")
    );
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn generates_the_complete_cross_platform_community_catalog() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-community-complete-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog fixture");
    let windows_package_manifest = windows_package_manifest_path();
    let windows_package_directory = root.join("windows-packages");
    let windows_manifest =
      create_manifest_packages(&windows_package_manifest, &windows_package_directory);
    let macos_package_manifest = macos_package_manifest_path();
    let macos_package_directory = root.join("macos-packages");
    let macos_manifest =
      create_manifest_packages(&macos_package_manifest, &macos_package_directory);

    let contents = generate_community_catalog(&CommunityCatalogInput {
      release_version: "0.1.12",
      catalog_sequence: 7,
      generated_at: "2026-08-31T14:31:13Z",
      expires_at: "2027-02-28T23:59:59Z",
      minimum_app_version: "0.1.12",
      windows_package_manifest: &windows_package_manifest,
      windows_package_directory: &windows_package_directory,
      macos_package_manifest: &macos_package_manifest,
      macos_package_directory: &macos_package_directory,
      now_unix_seconds: parse_rfc3339_utc("2026-08-31T14:32:00Z", "test").expect("parse test time"),
    })
    .expect("generate complete cross-platform Community Catalog");
    let catalog: RuntimeCatalog = serde_json::from_slice(&contents).expect("parse Catalog");

    assert_eq!(catalog.catalog_sequence, 7);
    assert_eq!(
      catalog.runtimes.len(),
      windows_manifest.packages.len() + macos_manifest.packages.len()
    );
    assert_eq!(
      catalog
        .runtimes
        .iter()
        .filter(|runtime| runtime.platform == "windows" && runtime.architecture == "x64")
        .count(),
      windows_manifest.packages.len()
    );
    assert_eq!(
      catalog
        .runtimes
        .iter()
        .filter(|runtime| runtime.platform == "macos" && runtime.architecture == "arm64")
        .count(),
      macos_manifest.packages.len()
    );
    assert!(catalog
      .runtimes
      .iter()
      .all(|runtime| runtime.url.contains("/releases/download/v0.1.12/")));
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn rejects_an_empty_macos_community_package() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-empty-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog fixture");
    let package_manifest = macos_package_manifest_path();
    let package_directory = root.join("packages");
    let manifest = create_manifest_packages(&package_manifest, &package_directory);
    let first_package = &manifest.packages[0];
    let empty_package = package_directory.join(format!(
      "{}-{}-macos-arm64-community.tar.gz",
      first_package.name, first_package.version
    ));
    std::fs::write(empty_package, []).expect("write empty macOS package");

    let error = generate_community_macos_catalog(&CommunityMacosCatalogInput {
      release_version: "0.1.4",
      catalog_sequence: 1,
      generated_at: "2026-08-30T00:00:00Z",
      expires_at: "2027-02-26T00:00:00Z",
      minimum_app_version: "0.1.4",
      package_manifest: &package_manifest,
      package_directory: &package_directory,
      now_unix_seconds: parse_rfc3339_utc("2026-08-30T00:01:00Z", "test").expect("parse test time"),
    })
    .expect_err("reject empty package");

    assert!(matches!(
      error,
      RuntimeCatalogBuildError::Catalog(RuntimeCatalogError::Invalid { ref field, .. })
        if field == "runtimes[0].size"
    ));
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn generates_and_parses_runtime_catalog_v1() {
    let contents = generate_runtime_catalog(&valid_catalog(), &catalog_validation(None))
      .expect("generate catalog");
    let document = serde_json::from_slice::<serde_json::Value>(&contents).expect("parse JSON");
    assert!(document["signature"].is_null());
    assert!(document["runtimes"][0]["signature"].is_null());

    let validated = parse_and_validate_runtime_catalog(&contents, &catalog_validation(None))
      .expect("validate catalog");
    assert_eq!(validated.catalog.catalog_sequence, 7);
    assert_eq!(validated.sha256.len(), 64);

    let accepted = AcceptedRuntimeCatalog {
      schema_version: validated.catalog.schema_version,
      sequence: validated.catalog.catalog_sequence,
      sha256: validated.sha256.clone(),
    };
    parse_and_validate_runtime_catalog(&contents, &catalog_validation(Some(&accepted)))
      .expect("accept identical catalog sequence and SHA-256");
  }

  #[test]
  fn accepts_runtime_catalog_v2_package_urls_from_catalog_releases() {
    let mut catalog = valid_catalog();
    catalog.schema_version = RUNTIME_CATALOG_SCHEMA_VERSION_V2;
    catalog.catalog_sequence = 16;
    catalog.runtimes[0].url = "https://github.com/JimmyWon1028/fabdev-runtimes/releases/download/catalog-v15/php-8.4.24-macos-arm64-community.tar.gz".to_owned();

    let contents = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect("generate Runtime Catalog v2");
    let validated = parse_and_validate_runtime_catalog(&contents, &catalog_validation(None))
      .expect("validate Runtime Catalog v2");
    assert_eq!(
      validated.catalog.schema_version,
      RUNTIME_CATALOG_SCHEMA_VERSION_V2
    );

    catalog.runtimes[0].url = "https://github.com/JimmyWon1028/fabdev-runtimes/releases/download/catalog-v0/php-8.4.24-macos-arm64-community.tar.gz".to_owned();
    assert!(generate_runtime_catalog(&catalog, &catalog_validation(None)).is_err());

    catalog.runtimes[0].url = "https://github.com/JimmyWon1028/fabdev-runtimes/releases/download/catalog-v17/php-8.4.24-macos-arm64-community.tar.gz".to_owned();
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject future Runtime Catalog Release URL");
    assert!(error.to_string().contains("future Runtime Catalog Release"));

    catalog.runtimes[0].url = "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.20/php-8.4.24-macos-arm64-community.tar.gz".to_owned();
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject App Release URL in Runtime Catalog v2");
    assert!(error.to_string().contains("fabdev-runtimes Release URL"));
  }

  #[test]
  fn generates_runtime_catalog_v2_from_a_full_runtime_index() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-v2-index-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog v2 fixture");
    let index_path = root.join("runtime-index-v1.json");
    let mut release = valid_catalog().runtimes.remove(0);
    release.url = "https://github.com/JimmyWon1028/fabdev-runtimes/releases/download/catalog-v12/php-8.4.24-macos-arm64-community.tar.gz".to_owned();
    std::fs::write(
      &index_path,
      serde_json::to_vec_pretty(&RuntimeCatalogIndex {
        schema_version: 1,
        runtimes: vec![release],
      })
      .expect("serialize Runtime index"),
    )
    .expect("write Runtime index");

    let contents = generate_community_catalog_v2(&CommunityCatalogV2Input {
      catalog_sequence: 13,
      generated_at: "2026-08-30T00:00:00Z",
      expires_at: "2027-02-26T00:00:00Z",
      minimum_app_version: "0.1.21",
      runtime_index: &index_path,
      now_unix_seconds: parse_rfc3339_utc("2026-08-30T00:01:00Z", "test").expect("parse test time"),
    })
    .expect("generate Runtime Catalog v2");
    let catalog = serde_json::from_slice::<RuntimeCatalog>(&contents).expect("parse Catalog v2");
    assert_eq!(catalog.schema_version, RUNTIME_CATALOG_SCHEMA_VERSION_V2);
    assert_eq!(catalog.catalog_sequence, 13);
    assert_eq!(catalog.compatibility.minimum_app_version, "0.1.21");
    assert_eq!(
      catalog.compatibility.minimum_agent_protocol_version,
      COMMUNITY_RUNTIME_CATALOG_V2_MINIMUM_PROTOCOL_VERSION
    );
    assert_eq!(catalog.runtimes.len(), 1);
    std::fs::remove_dir_all(root).expect("remove Catalog v2 fixture");
  }

  #[test]
  fn accepts_windows_x64_runtime_with_official_sha256_source() {
    let mut catalog = valid_catalog();
    let release = &mut catalog.runtimes[0];
    release.platform = "windows".to_owned();
    release.architecture = "x64".to_owned();
    release.minimum_os_version = Some("10.0".to_owned());
    release.file_name = Some("php-8.4.24-windows-x64-community.tar.gz".to_owned());
    release.url = "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-8.4.24-windows-x64-community.tar.gz".to_owned();
    release.source_verification = Some(RuntimeSourceVerification {
      method: "official-sha256".to_owned(),
      fingerprint: None,
      upstream_sha256: "b".repeat(64),
    });

    generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect("accept Windows x64 Runtime");
  }

  #[test]
  fn accepts_future_php_series_on_both_platforms() {
    let mut catalog = valid_catalog();
    let release = &mut catalog.runtimes[0];
    release.version = "9.1.2".to_owned();
    release.platform = "windows".to_owned();
    release.architecture = "x64".to_owned();
    release.minimum_os_version = Some("11.0".to_owned());
    release.file_name = Some("php-9.1.2-windows-x64-community.tar.gz".to_owned());
    release.url = "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-9.1.2-windows-x64-community.tar.gz".to_owned();
    release.source_verification = Some(RuntimeSourceVerification {
      method: "official-sha256".to_owned(),
      fingerprint: None,
      upstream_sha256: "b".repeat(64),
    });
    generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect("accept future Windows PHP Runtime");

    let release = &mut catalog.runtimes[0];
    release.platform = "macos".to_owned();
    release.architecture = "arm64".to_owned();
    release.file_name = Some("php-9.1.2-macos-arm64-community.tar.gz".to_owned());
    release.url = "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-9.1.2-macos-arm64-community.tar.gz".to_owned();
    generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect("accept future macOS PHP Runtime");
  }

  #[test]
  fn rejects_catalog_rollback_and_reused_sequence() {
    let original = generate_runtime_catalog(&valid_catalog(), &catalog_validation(None))
      .expect("generate original catalog");
    let original = parse_and_validate_runtime_catalog(&original, &catalog_validation(None))
      .expect("validate original catalog");

    let accepted_newer = AcceptedRuntimeCatalog {
      schema_version: original.catalog.schema_version,
      sequence: original.catalog.catalog_sequence + 1,
      sha256: "c".repeat(64),
    };
    let rollback =
      generate_runtime_catalog(&valid_catalog(), &catalog_validation(Some(&accepted_newer)))
        .expect_err("reject sequence rollback");
    assert!(matches!(
      rollback,
      RuntimeCatalogError::SequenceRollback { .. }
    ));

    let accepted_changed = AcceptedRuntimeCatalog {
      schema_version: original.catalog.schema_version,
      sequence: original.catalog.catalog_sequence,
      sha256: "d".repeat(64),
    };
    let reused = parse_and_validate_runtime_catalog(
      &serde_json::to_vec(&valid_catalog()).expect("serialize changed catalog"),
      &catalog_validation(Some(&accepted_changed)),
    )
    .expect_err("reject reused sequence with different contents");
    assert!(matches!(
      reused,
      RuntimeCatalogError::SequenceHashMismatch { .. }
    ));
  }

  #[test]
  fn rejects_expired_incompatible_and_unofficial_catalogs() {
    let mut catalog = valid_catalog();
    catalog.expires_at = "2026-08-30T00:00:30Z".to_owned();
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject expired catalog");
    assert!(error.to_string().contains("catalog is expired"));

    let mut catalog = valid_catalog();
    catalog.compatibility.minimum_app_version = "0.2.0".to_owned();
    assert!(matches!(
      generate_runtime_catalog(&catalog, &catalog_validation(None))
        .expect_err("reject incompatible app"),
      RuntimeCatalogError::IncompatibleApp { .. }
    ));

    let catalog = valid_catalog();
    let incompatible_protocol = RuntimeCatalogValidation {
      current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION - 1,
      ..catalog_validation(None)
    };
    assert!(matches!(
      generate_runtime_catalog(&catalog, &incompatible_protocol)
        .expect_err("reject incompatible Agent Protocol"),
      RuntimeCatalogError::IncompatibleProtocol { .. }
    ));

    let mut catalog = valid_catalog();
    catalog.runtimes[0].url = "https://example.invalid/php.tar.gz".to_owned();
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject unofficial URL");
    assert!(error
      .to_string()
      .contains("official versioned GitHub Release URL"));
  }

  #[test]
  fn rejects_claimed_signature_duplicate_runtime_and_invalid_sha256() {
    let mut catalog = valid_catalog();
    catalog.runtimes[0].signature = Some("community-ad-hoc".to_owned());
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject claimed package signature");
    assert!(error.to_string().contains("must be null"));

    let mut catalog = valid_catalog();
    catalog.runtimes.push(catalog.runtimes[0].clone());
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject duplicate Runtime identity");
    assert!(error.to_string().contains("duplicates"));

    let mut catalog = valid_catalog();
    catalog.runtimes[0].sha256 = "A".repeat(64);
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject uppercase SHA-256");
    assert!(error.to_string().contains("lowercase hexadecimal"));

    let mut catalog = valid_catalog();
    catalog.catalog_sequence = MAX_SAFE_JSON_INTEGER + 1;
    let error = generate_runtime_catalog(&catalog, &catalog_validation(None))
      .expect_err("reject Catalog sequence outside the JSON safe integer range");
    assert!(error.to_string().contains("must be between 1"));
  }

  #[test]
  fn rejects_missing_runtime_signature_and_oversized_catalog() {
    let contents = generate_runtime_catalog(&valid_catalog(), &catalog_validation(None))
      .expect("generate catalog");
    let mut document = serde_json::from_slice::<serde_json::Value>(&contents).expect("parse JSON");
    document["runtimes"][0]
      .as_object_mut()
      .expect("Runtime object")
      .remove("signature");
    let contents = serde_json::to_vec(&document).expect("serialize JSON");
    let error = parse_and_validate_runtime_catalog(&contents, &catalog_validation(None))
      .expect_err("reject missing Runtime signature");
    assert!(error.to_string().contains("must be present and null"));

    let oversized = vec![b' '; RUNTIME_CATALOG_MAX_BYTES + 1];
    assert!(matches!(
      parse_and_validate_runtime_catalog(&oversized, &catalog_validation(None))
        .expect_err("reject oversized catalog"),
      RuntimeCatalogError::TooLarge
    ));
  }

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
  fn retries_only_permission_denied_filesystem_operations() {
    let mut attempts = 0;
    let result = retry_permission_denied(3, Duration::ZERO, || {
      attempts += 1;
      if attempts < 3 {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
      } else {
        Ok("renamed")
      }
    })
    .expect("retry transient permission denial");
    assert_eq!(result, "renamed");
    assert_eq!(attempts, 3);

    let mut non_retryable_attempts = 0;
    let error = retry_permission_denied(3, Duration::ZERO, || {
      non_retryable_attempts += 1;
      Err::<(), _>(std::io::Error::from(std::io::ErrorKind::NotFound))
    })
    .expect_err("return non-retryable error");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(non_retryable_attempts, 1);
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
  fn runs_health_check_before_install_and_cleans_failed_staging() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-health-{}", uuid::Uuid::new_v4()));
    let source = root.join("source/8.4.24");
    std::fs::create_dir_all(&source).expect("create Runtime fixture");
    std::fs::write(source.join("marker.txt"), b"healthy").expect("write Runtime fixture");
    let artifact = root.join("runtime.tar.gz");
    let archive_file = File::create(&artifact).expect("create Runtime archive");
    let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    archive
      .append_dir_all("8.4.24", &source)
      .expect("append Runtime fixture");
    archive.finish().expect("finish Runtime archive");
    drop(archive);
    let checksum = hex::encode(Sha256::digest(
      std::fs::read(&artifact).expect("read archive"),
    ));
    let runtime_base = root.join("runtimes");
    std::fs::create_dir_all(runtime_base.join("php/8.2.33")).expect("create existing Runtime");
    set_active_version(&runtime_base, "php", "8.2.33").expect("activate existing Runtime");

    let error = install_tar_gz_with_health_check(
      &artifact,
      &checksum,
      "php",
      "8.4.24",
      &runtime_base,
      false,
      |staged| {
        assert_eq!(
          std::fs::read(staged.join("marker.txt")).expect("read staged fixture"),
          b"healthy"
        );
        Err(RuntimeError::HealthCheckFailed(
          "fixture failure".to_owned(),
        ))
      },
    )
    .expect_err("reject unhealthy Runtime");

    assert!(matches!(error, RuntimeError::HealthCheckFailed(_)));
    assert!(!runtime_base.join("php/8.4.24").exists());
    assert!(!runtime_base.join(".staging/php-8.4.24").exists());

    install_tar_gz_with_health_check(
      &artifact,
      &checksum,
      "php",
      "8.4.24",
      &runtime_base,
      false,
      |_| Ok(()),
    )
    .expect("install healthy Runtime side by side");
    assert!(runtime_base.join("php/8.4.24").is_dir());
    assert_eq!(
      active_version(&runtime_base, "php").expect("read active Runtime"),
      Some("8.2.33".to_owned())
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  fn removal_preference_fixture() -> (PathBuf, PathBuf, String) {
    let root =
      std::env::temp_dir().join(format!("fabdev-removal-rollback-{}", uuid::Uuid::new_v4()));
    let source = root.join("source/8.2.33");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("marker.txt"), b"fixture").unwrap();
    let artifact = root.join("fixture.tar.gz");
    let encoder = flate2::write::GzEncoder::new(
      File::create(&artifact).unwrap(),
      flate2::Compression::default(),
    );
    let mut archive = tar::Builder::new(encoder);
    archive.append_dir_all("8.2.33", &source).unwrap();
    archive.into_inner().unwrap().finish().unwrap();
    let checksum = hex::encode(Sha256::digest(std::fs::read(&artifact).unwrap()));
    (root, artifact, checksum)
  }

  #[test]
  fn rollback_preserves_runtime_removal_preference_and_previous_state() {
    for was_removed in [true, false] {
      for activate in [false, true] {
        let (root, artifact, checksum) = removal_preference_fixture();
        let base = root.join("runtimes");
        std::fs::create_dir_all(base.join("php/7.4.33")).unwrap();
        std::fs::write(base.join("php/7.4.33/previous.txt"), b"previous").unwrap();
        std::fs::write(root.join("user-settings.ini"), b"memory_limit=512M").unwrap();
        set_active_version(&base, "php", "7.4.33").unwrap();
        if was_removed {
          mark_runtime_removed(&base, "php", "8.2.33").unwrap();
        }
        let transaction = install_or_replace_tar_gz_with_health_check(
          RuntimePackageInstallInput {
            artifact: &artifact,
            expected_sha256: &checksum,
            catalog_sequence: 1,
            name: "php",
            version: "8.2.33",
            base: &base,
            activate,
          },
          |_| Ok(()),
        )
        .unwrap();
        rollback_runtime_install_transaction(transaction).unwrap();
        let removed_after = is_runtime_marked_removed(&base, "php", "8.2.33").unwrap();
        assert_eq!(
          active_version(&base, "php").unwrap().as_deref(),
          Some("7.4.33")
        );
        assert_eq!(
          std::fs::read(base.join("php/7.4.33/previous.txt")).unwrap(),
          b"previous"
        );
        assert_eq!(
          std::fs::read(root.join("user-settings.ini")).unwrap(),
          b"memory_limit=512M"
        );
        assert!(!base.join("php/8.2.33").exists());
        assert!(!base.join(".staging/php-8.2.33").exists());
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(
          removed_after, was_removed,
          "rollback must preserve the user's explicit removal choice"
        );
      }
    }
  }

  #[test]
  fn successful_install_clears_runtime_removal_preference() {
    let (root, artifact, checksum) = removal_preference_fixture();
    let base = root.join("runtimes");
    mark_runtime_removed(&base, "php", "8.2.33").unwrap();
    let transaction = install_or_replace_tar_gz_with_health_check(
      RuntimePackageInstallInput {
        artifact: &artifact,
        expected_sha256: &checksum,
        catalog_sequence: 1,
        name: "php",
        version: "8.2.33",
        base: &base,
        activate: true,
      },
      |_| Ok(()),
    )
    .unwrap();
    commit_runtime_install_transaction(transaction).unwrap();
    assert!(!is_runtime_marked_removed(&base, "php", "8.2.33").unwrap());
    assert_eq!(
      active_version(&base, "php").unwrap().as_deref(),
      Some("8.2.33")
    );
    assert_eq!(
      std::fs::read(base.join("php/8.2.33/marker.txt")).unwrap(),
      b"fixture"
    );
    std::fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn failed_health_check_preserves_runtime_removal_preference() {
    let (root, artifact, checksum) = removal_preference_fixture();
    let base = root.join("runtimes");
    mark_runtime_removed(&base, "php", "8.2.33").unwrap();
    let result = install_or_replace_tar_gz_with_health_check(
      RuntimePackageInstallInput {
        artifact: &artifact,
        expected_sha256: &checksum,
        catalog_sequence: 1,
        name: "php",
        version: "8.2.33",
        base: &base,
        activate: true,
      },
      |_| {
        Err(RuntimeError::HealthCheckFailed(
          "fixture failure".to_owned(),
        ))
      },
    );
    assert!(result.is_err());
    assert!(is_runtime_marked_removed(&base, "php", "8.2.33").unwrap());
    assert!(!base.join("php/8.2.33").exists());
    assert!(!base.join(".staging/php-8.2.33").exists());
    std::fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn replaces_same_version_by_package_sha_and_supports_rollback() {
    fn build_archive(root: &Path, label: &str, payload: &[u8]) -> (PathBuf, String) {
      let source = root.join(format!("source-{label}/8.4.24"));
      std::fs::create_dir_all(&source).expect("create replacement source");
      std::fs::write(source.join("marker.txt"), payload).expect("write replacement payload");
      let artifact = root.join(format!("runtime-{label}.tar.gz"));
      let archive_file = File::create(&artifact).expect("create replacement archive");
      let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
      let mut archive = tar::Builder::new(encoder);
      archive
        .append_dir_all("8.4.24", &source)
        .expect("append replacement fixture");
      archive.finish().expect("finish replacement archive");
      drop(archive);
      let sha256 = hex::encode(Sha256::digest(
        std::fs::read(&artifact).expect("read replacement archive"),
      ));
      (artifact, sha256)
    }

    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-replacement-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create replacement fixture");
    let runtime_base = root.join("runtimes");
    let (original_artifact, original_sha256) = build_archive(&root, "original", b"original");
    let original = install_or_replace_tar_gz_with_health_check(
      RuntimePackageInstallInput {
        artifact: &original_artifact,
        expected_sha256: &original_sha256,
        catalog_sequence: 1,
        name: "php",
        version: "8.4.24",
        base: &runtime_base,
        activate: false,
      },
      |_| Ok(()),
    )
    .expect("install original package");
    commit_runtime_install_transaction(original).expect("commit original package");

    let (replacement_artifact, replacement_sha256) =
      build_archive(&root, "replacement", b"replacement");
    let replacement = install_or_replace_tar_gz_with_health_check(
      RuntimePackageInstallInput {
        artifact: &replacement_artifact,
        expected_sha256: &replacement_sha256,
        catalog_sequence: 2,
        name: "php",
        version: "8.4.24",
        base: &runtime_base,
        activate: false,
      },
      |_| Ok(()),
    )
    .expect("stage replacement package");
    assert_eq!(
      installed_runtime_package_sha256(&runtime_base, "php", "8.4.24")
        .expect("read replacement receipt"),
      Some(replacement_sha256.clone())
    );
    rollback_runtime_install_transaction(replacement).expect("roll back replacement package");
    assert_eq!(
      std::fs::read(runtime_base.join("php/8.4.24/marker.txt")).expect("read restored payload"),
      b"original"
    );
    assert_eq!(
      installed_runtime_package_sha256(&runtime_base, "php", "8.4.24")
        .expect("read restored receipt"),
      Some(original_sha256)
    );

    let replacement = install_or_replace_tar_gz_with_health_check(
      RuntimePackageInstallInput {
        artifact: &replacement_artifact,
        expected_sha256: &replacement_sha256,
        catalog_sequence: 2,
        name: "php",
        version: "8.4.24",
        base: &runtime_base,
        activate: false,
      },
      |_| Ok(()),
    )
    .expect("replace original package");
    commit_runtime_install_transaction(replacement).expect("commit replacement package");
    assert_eq!(
      std::fs::read(runtime_base.join("php/8.4.24/marker.txt")).expect("read committed payload"),
      b"replacement"
    );
    assert!(!runtime_base.join("php/.8.4.24.fabdev-backup").exists());
    std::fs::remove_dir_all(root).expect("remove replacement fixture");
  }

  #[cfg(windows)]
  #[test]
  #[ignore = "requires the verified Windows PHP package built by the release workflow"]
  fn installs_real_windows_php_archive() {
    let manifest_path = PathBuf::from(
      std::env::var("FABDEV_WINDOWS_RUNTIME_PACKAGE_MANIFEST")
        .expect("FABDEV_WINDOWS_RUNTIME_PACKAGE_MANIFEST must identify the package manifest"),
    );
    let package_directory = PathBuf::from(
      std::env::var("FABDEV_WINDOWS_RUNTIME_PACKAGE_DIR")
        .expect("FABDEV_WINDOWS_RUNTIME_PACKAGE_DIR must identify the package directory"),
    );
    let manifest = read_runtime_package_manifest(&manifest_path).expect("read package manifest");
    for package in manifest
      .packages
      .iter()
      .filter(|package| package.name == "php")
    {
      let version = package.version.as_str();
      let artifact = package_directory.join(format!(
        "{}-{}-{}-{}-community.tar.gz",
        package.name, package.version, manifest.platform, manifest.architecture
      ));
      let checksum = hex::encode(Sha256::digest(
        std::fs::read(&artifact).expect("read Windows PHP Runtime package"),
      ));
      let root = std::env::temp_dir().join(format!(
        "fabdev-runtime-windows-package-{}",
        uuid::Uuid::new_v4()
      ));
      mark_runtime_removed(&root, "php", version).expect("mark PHP Runtime removed");
      assert!(is_runtime_marked_removed(&root, "php", version).expect("read removed marker"));

      install_tar_gz_with_activation(&artifact, &checksum, "php", version, &root, false)
        .expect("install packaged Windows PHP Runtime");

      assert!(root.join("php").join(version).join("php.exe").is_file());
      assert!(root.join("php").join(version).join("php-cgi.exe").is_file());
      assert!(!root.join("php/current.version").exists());
      assert!(
        !is_runtime_marked_removed(&root, "php", version).expect("verify removed marker cleared")
      );
      std::fs::remove_dir_all(root).expect("remove Windows Runtime fixture");
    }
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
  #[cfg(unix)]
  fn rejects_switching_to_runtime_links_without_changing_current() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("fabdev-runtime-link-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("php/8.2.33")).unwrap();
    std::fs::create_dir_all(root.join("external")).unwrap();
    symlink("8.2.33", root.join("php/alias")).unwrap();
    symlink(root.join("external"), root.join("php/external")).unwrap();
    symlink("missing", root.join("php/broken")).unwrap();
    let mut accepted = Vec::new();
    for version in ["current", "alias", "external", "broken"] {
      set_active_version(&root, "php", "8.2.33").unwrap();
      let result = set_active_version(&root, "php", version);
      if result.is_ok() {
        accepted.push(version);
        continue;
      }
      assert!(matches!(result, Err(RuntimeError::NotInstalled(_, _))));
      assert_eq!(
        active_version(&root, "php").unwrap().as_deref(),
        Some("8.2.33")
      );
      assert_eq!(
        std::fs::read_link(root.join("php/current")).unwrap(),
        PathBuf::from("8.2.33")
      );
    }
    std::fs::remove_dir_all(root).unwrap();
    assert!(
      accepted.is_empty(),
      "switch accepted Runtime links: {accepted:?}"
    );
  }

  #[test]
  fn failed_runtime_switch_preserves_the_selected_version() {
    let root = std::env::temp_dir().join(format!("fabdev-runtime-switch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("node/24.20.0")).unwrap();
    std::fs::write(root.join("node/24.19.0"), "not a Runtime directory").unwrap();
    set_active_version(&root, "node", "24.20.0").unwrap();
    for version in ["24.19.0", "missing", "../external"] {
      assert!(set_active_version(&root, "node", version).is_err());
      assert_eq!(
        active_version(&root, "node").unwrap().as_deref(),
        Some("24.20.0")
      );
    }
    std::fs::create_dir_all(root.join("node/20.20.0")).unwrap();
    set_active_version(&root, "node", "20.20.0").unwrap();
    assert_eq!(
      active_version(&root, "node").unwrap().as_deref(),
      Some("20.20.0")
    );
    std::fs::remove_dir_all(root).unwrap();
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
