use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const RUNTIME_CATALOG_MAX_BYTES: usize = 1024 * 1024;
pub const RUNTIME_CATALOG_PRODUCT: &str = "fabdev-runtime";
pub const RUNTIME_CATALOG_CHANNEL: &str = "community";
pub const RUNTIME_CATALOG_SCHEMA_VERSION: u16 = 1;
pub const RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION: u16 = 33;
pub const RUNTIME_CATALOG_URL: &str =
  "https://github.com/JimmyWon1028/fabdev/releases/latest/download/fabdev-runtime-v1.json";

const RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/JimmyWon1028/fabdev/releases/download/v";
const MAX_GENERATED_AT_FUTURE_SECONDS: i64 = 5 * 60;
const MAX_PHP_RUNTIME_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
const PHP_SOURCE_SIGNING_FINGERPRINT: &str = "9D7F99A0CB8F05C8A6958D6256A97AF7600A39A6";
const PHP_84_MACOS_SOURCE_SHA256: &str =
  "e127be09a8506f4327c5cfa78a614b00d210714484ec215ce0011b4a03c00731";
const PHP_84_WINDOWS_SOURCE_SHA256: &str =
  "86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb";

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRuntimeCatalog {
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
pub struct CommunityPhpCatalogInput<'a> {
  pub release_version: &'a str,
  pub catalog_sequence: u64,
  pub generated_at: &'a str,
  pub expires_at: &'a str,
  pub minimum_app_version: &'a str,
  pub macos_arm64_package: &'a Path,
  pub windows_x64_package: &'a Path,
  pub now_unix_seconds: i64,
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
  #[error("runtime health check failed: {0}")]
  HealthCheckFailed(String),
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

pub fn generate_community_php_catalog(
  input: &CommunityPhpCatalogInput<'_>,
) -> Result<Vec<u8>, RuntimeCatalogBuildError> {
  let macos_file_name = "php-8.4.24-macos-arm64-community.tar.gz";
  let windows_file_name = "php-8.4.24-windows-x64-community.tar.gz";
  let (macos_size, macos_sha256) = file_size_and_sha256(input.macos_arm64_package)?;
  let (windows_size, windows_sha256) = file_size_and_sha256(input.windows_x64_package)?;
  let release_url = |file_name: &str| {
    format!(
      "{RELEASE_DOWNLOAD_PREFIX}{}/{file_name}",
      input.release_version
    )
  };
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
      minimum_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    },
    signature: None,
    runtimes: vec![
      RuntimeRelease {
        name: "php".to_owned(),
        version: "8.4.24".to_owned(),
        platform: "macos".to_owned(),
        architecture: "arm64".to_owned(),
        minimum_os_version: Some("13.0".to_owned()),
        file_name: Some(macos_file_name.to_owned()),
        url: release_url(macos_file_name),
        size: macos_size,
        sha256: macos_sha256,
        signature: None,
        source_verification: Some(RuntimeSourceVerification {
          method: "pgp".to_owned(),
          fingerprint: Some(PHP_SOURCE_SIGNING_FINGERPRINT.to_owned()),
          upstream_sha256: PHP_84_MACOS_SOURCE_SHA256.to_owned(),
        }),
        archive_format: Some("tar.gz".to_owned()),
        install_mode: Some("side-by-side".to_owned()),
        health_check_profile: Some("php-runtime-v1".to_owned()),
      },
      RuntimeRelease {
        name: "php".to_owned(),
        version: "8.4.24".to_owned(),
        platform: "windows".to_owned(),
        architecture: "x64".to_owned(),
        minimum_os_version: Some("11.0".to_owned()),
        file_name: Some(windows_file_name.to_owned()),
        url: release_url(windows_file_name),
        size: windows_size,
        sha256: windows_sha256,
        signature: None,
        source_verification: Some(RuntimeSourceVerification {
          method: "official-sha256".to_owned(),
          fingerprint: None,
          upstream_sha256: PHP_84_WINDOWS_SOURCE_SHA256.to_owned(),
        }),
        archive_format: Some("tar.gz".to_owned()),
        install_mode: Some("side-by-side".to_owned()),
        health_check_profile: Some("php-runtime-v1".to_owned()),
      },
    ],
  };
  let validation = RuntimeCatalogValidation {
    current_app_version: input.minimum_app_version,
    current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    now_unix_seconds: input.now_unix_seconds,
    accepted_catalog: None,
  };
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
    catalog.schema_version == RUNTIME_CATALOG_SCHEMA_VERSION,
    "schemaVersion",
    format!("must be {RUNTIME_CATALOG_SCHEMA_VERSION}"),
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
      == RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
    "compatibility.minimumAgentProtocolVersion",
    format!("must be {RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION} for schema v1"),
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
    validate_catalog_release(release, index)?;
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
) -> Result<(), RuntimeCatalogError> {
  let field = |name: &str| format!("runtimes[{index}].{name}");
  require_catalog_value(release.name == "php", field("name"), "must be php")?;
  require_catalog_value(
    release.version == "8.4.24",
    field("version"),
    "must be 8.4.24 for Community v1",
  )?;
  let supported_target = matches!(
    (release.platform.as_str(), release.architecture.as_str()),
    ("macos", "arm64") | ("windows", "x64")
  );
  require_catalog_value(
    supported_target,
    field("platform"),
    "must target macos/arm64 or windows/x64",
  )?;
  let minimum_os_version = release
    .minimum_os_version
    .as_deref()
    .ok_or_else(|| invalid_catalog(field("minimumOsVersion"), "is required"))?;
  validate_numeric_version(minimum_os_version, &field("minimumOsVersion"))?;

  let expected_file_name = format!(
    "php-{}-{}-{}-community.tar.gz",
    release.version, release.platform, release.architecture
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
  validate_versioned_release_url(&release.url, file_name, &field("url"))?;
  require_catalog_value(
    release.size > 0 && release.size <= MAX_PHP_RUNTIME_BYTES,
    field("size"),
    format!("must be between 1 and {MAX_PHP_RUNTIME_BYTES} bytes"),
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
  match source.method.as_str() {
    "pgp" => require_catalog_value(
      source.fingerprint.as_deref() == Some(PHP_SOURCE_SIGNING_FINGERPRINT),
      field("sourceVerification.fingerprint"),
      format!("must be {PHP_SOURCE_SIGNING_FINGERPRINT}"),
    )?,
    "official-sha256" => require_catalog_value(
      source.fingerprint.is_none(),
      field("sourceVerification.fingerprint"),
      "must be omitted for official-sha256",
    )?,
    _ => {
      return Err(invalid_catalog(
        field("sourceVerification.method"),
        "must be pgp or official-sha256",
      ));
    }
  }
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
    "php-runtime-v1",
    field("healthCheckProfile"),
  )?;
  Ok(())
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
          fingerprint: Some(PHP_SOURCE_SIGNING_FINGERPRINT.to_owned()),
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
  fn generates_the_fixed_community_php_catalog_from_packages() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-build-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog fixture");
    let macos_package = root.join("macos.tar.gz");
    let windows_package = root.join("windows.tar.gz");
    std::fs::write(&macos_package, b"macos runtime").expect("write macOS package");
    std::fs::write(&windows_package, b"windows runtime").expect("write Windows package");

    let contents = generate_community_php_catalog(&CommunityPhpCatalogInput {
      release_version: "0.1.4",
      catalog_sequence: 1,
      generated_at: "2026-08-30T00:00:00Z",
      expires_at: "2027-02-26T00:00:00Z",
      minimum_app_version: "0.1.4",
      macos_arm64_package: &macos_package,
      windows_x64_package: &windows_package,
      now_unix_seconds: parse_rfc3339_utc("2026-08-30T00:01:00Z", "test").expect("parse test time"),
    })
    .expect("generate Community PHP Catalog");
    let catalog: RuntimeCatalog = serde_json::from_slice(&contents).expect("parse Catalog");

    assert_eq!(catalog.catalog_sequence, 1);
    assert_eq!(catalog.signature, None);
    assert_eq!(catalog.runtimes.len(), 2);
    assert_eq!(catalog.runtimes[0].platform, "macos");
    assert_eq!(catalog.runtimes[0].size, 13);
    assert_eq!(
      catalog.runtimes[0].sha256,
      hex::encode(Sha256::digest(b"macos runtime"))
    );
    assert_eq!(
      catalog.runtimes[0]
        .source_verification
        .as_ref()
        .expect("macOS source verification")
        .fingerprint
        .as_deref(),
      Some(PHP_SOURCE_SIGNING_FINGERPRINT)
    );
    assert_eq!(catalog.runtimes[1].platform, "windows");
    assert_eq!(
      catalog.runtimes[1].minimum_os_version.as_deref(),
      Some("11.0")
    );
    assert_eq!(catalog.runtimes[1].size, 15);
    assert_eq!(
      catalog.runtimes[1].url,
      "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-8.4.24-windows-x64-community.tar.gz"
    );
    std::fs::remove_dir_all(root).expect("remove Catalog fixture");
  }

  #[test]
  fn rejects_an_empty_community_php_package() {
    let root = std::env::temp_dir().join(format!(
      "fabdev-runtime-catalog-empty-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Catalog fixture");
    let macos_package = root.join("macos.tar.gz");
    let windows_package = root.join("windows.tar.gz");
    std::fs::write(&macos_package, []).expect("write empty macOS package");
    std::fs::write(&windows_package, b"windows runtime").expect("write Windows package");

    let error = generate_community_php_catalog(&CommunityPhpCatalogInput {
      release_version: "0.1.4",
      catalog_sequence: 1,
      generated_at: "2026-08-30T00:00:00Z",
      expires_at: "2027-02-26T00:00:00Z",
      minimum_app_version: "0.1.4",
      macos_arm64_package: &macos_package,
      windows_x64_package: &windows_package,
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
      sequence: validated.catalog.catalog_sequence,
      sha256: validated.sha256.clone(),
    };
    parse_and_validate_runtime_catalog(&contents, &catalog_validation(Some(&accepted)))
      .expect("accept identical catalog sequence and SHA-256");
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
  fn rejects_catalog_rollback_and_reused_sequence() {
    let original = generate_runtime_catalog(&valid_catalog(), &catalog_validation(None))
      .expect("generate original catalog");
    let original = parse_and_validate_runtime_catalog(&original, &catalog_validation(None))
      .expect("validate original catalog");

    let accepted_newer = AcceptedRuntimeCatalog {
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
