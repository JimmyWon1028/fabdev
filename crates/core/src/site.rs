use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhpVersion {
  pub major: u8,
  pub minor: u8,
}

impl Display for PhpVersion {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}.{}", self.major, self.minor)
  }
}

impl FromStr for PhpVersion {
  type Err = SiteError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let mut parts = value.split('.');
    let major = parts
      .next()
      .and_then(|part| part.parse::<u8>().ok())
      .ok_or_else(|| SiteError::InvalidPhpVersion(value.to_owned()))?;
    let minor = parts
      .next()
      .and_then(|part| part.parse::<u8>().ok())
      .ok_or_else(|| SiteError::InvalidPhpVersion(value.to_owned()))?;
    if parts.next().is_some() || major < 7 {
      return Err(SiteError::InvalidPhpVersion(value.to_owned()));
    }
    Ok(Self { major, minor })
  }
}

impl Serialize for PhpVersion {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    serializer.serialize_str(&self.to_string())
  }
}

impl<'de> Deserialize<'de> for PhpVersion {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInput {
  pub name: Option<String>,
  pub domain: Option<String>,
  pub project_path: PathBuf,
  pub document_root: Option<PathBuf>,
  pub php_version: Option<PhpVersion>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteEditInput {
  pub name: String,
  pub domain: String,
  pub project_path: PathBuf,
  pub document_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
  pub id: Uuid,
  pub name: String,
  pub domain: String,
  pub project_path: PathBuf,
  pub document_root: PathBuf,
  pub php_version: Option<PhpVersion>,
  pub enabled: bool,
  #[serde(default)]
  pub secured: bool,
}

#[derive(Debug, Error)]
pub enum SiteError {
  #[error("project path does not exist: {0}")]
  MissingProject(PathBuf),
  #[error("project path is not a directory: {0}")]
  ProjectIsNotDirectory(PathBuf),
  #[error("document root must be inside the project directory")]
  DocumentRootOutsideProject,
  #[error("invalid .test domain: {0}")]
  InvalidDomain(String),
  #[error("invalid PHP version: {0}")]
  InvalidPhpVersion(String),
  #[error("project directory does not have a valid name")]
  MissingProjectName,
  #[error("Site name cannot be empty")]
  EmptyName,
  #[error("unable to resolve project path: {0}")]
  ResolvePath(#[from] std::io::Error),
}

pub fn create_site(input: SiteInput) -> Result<Site, SiteError> {
  if !input.project_path.exists() {
    return Err(SiteError::MissingProject(input.project_path));
  }
  if !input.project_path.is_dir() {
    return Err(SiteError::ProjectIsNotDirectory(input.project_path));
  }

  let project_path = input.project_path.canonicalize()?;
  let fallback_name = project_path
    .file_name()
    .and_then(|name| name.to_str())
    .filter(|name| !name.is_empty())
    .ok_or(SiteError::MissingProjectName)?;
  let name = input
    .name
    .filter(|name| !name.trim().is_empty())
    .unwrap_or_else(|| fallback_name.to_owned());
  let domain = normalize_domain(
    &input
      .domain
      .unwrap_or_else(|| default_site_domain(fallback_name)),
  )?;
  let document_root = match input.document_root {
    Some(path) => {
      let candidate = if path.is_absolute() {
        path
      } else {
        project_path.join(path)
      };
      let candidate = candidate.canonicalize()?;
      if !candidate.starts_with(&project_path) {
        return Err(SiteError::DocumentRootOutsideProject);
      }
      candidate
    }
    None => detect_document_root(&project_path),
  };

  Ok(Site {
    id: Uuid::new_v4(),
    name,
    domain,
    project_path,
    document_root,
    php_version: input.php_version,
    enabled: true,
    secured: false,
  })
}

pub fn edit_site(previous: &Site, input: SiteEditInput) -> Result<Site, SiteError> {
  let name = input.name.trim();
  if name.is_empty() {
    return Err(SiteError::EmptyName);
  }
  let mut updated = create_site(SiteInput {
    name: Some(name.to_owned()),
    domain: Some(input.domain),
    project_path: input.project_path,
    document_root: input.document_root,
    php_version: previous.php_version.clone(),
  })?;
  updated.id = previous.id;
  updated.enabled = previous.enabled;
  updated.secured = previous.secured;
  Ok(updated)
}

pub fn detect_document_root(project_path: &Path) -> PathBuf {
  let public = project_path.join("public");
  if public.join("index.php").is_file() || public.join("index.html").is_file() {
    public
  } else {
    project_path.to_path_buf()
  }
}

pub fn normalize_domain(value: &str) -> Result<String, SiteError> {
  let domain = value.trim().trim_end_matches('.').to_ascii_lowercase();
  if !domain.ends_with(".test") || domain.len() > 253 {
    return Err(SiteError::InvalidDomain(value.to_owned()));
  }
  let valid = domain.split('.').all(|label| {
    !label.is_empty()
      && label.len() <= 63
      && !label.starts_with('-')
      && !label.ends_with('-')
      && label
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
  });
  if !valid {
    return Err(SiteError::InvalidDomain(value.to_owned()));
  }
  Ok(domain)
}

fn slugify(value: &str) -> String {
  let slug = value
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() {
        character.to_ascii_lowercase()
      } else {
        '-'
      }
    })
    .collect::<String>();
  let slug = slug.trim_matches('-');
  if slug.is_empty() {
    "site".to_owned()
  } else {
    slug.to_owned()
  }
}

pub fn default_site_domain(value: &str) -> String {
  format!("{}.test", slugify(value))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn temp_project() -> PathBuf {
    let root = std::env::temp_dir().join(format!("fabdev-site-{}", Uuid::new_v4()));
    std::fs::create_dir_all(root.join("public")).expect("create test project");
    std::fs::write(root.join("public/index.php"), "<?php echo 'ok';")
      .expect("write test front controller");
    root
  }

  #[test]
  fn parses_supported_php_version_shape() {
    assert_eq!(
      "8.2"
        .parse::<PhpVersion>()
        .expect("parse version")
        .to_string(),
      "8.2"
    );
    assert!("8.2.33".parse::<PhpVersion>().is_err());
  }

  #[test]
  fn detects_public_front_controller() {
    let root = temp_project();
    assert_eq!(detect_document_root(&root), root.join("public"));
    std::fs::remove_dir_all(root).expect("remove test project");
  }

  #[test]
  fn detects_static_public_document_root() {
    let root = std::env::temp_dir().join(format!("fabdev-static-site-{}", Uuid::new_v4()));
    std::fs::create_dir_all(root.join("public")).expect("create static project");
    std::fs::write(root.join("public/index.html"), "static")
      .expect("write static front controller");
    assert_eq!(detect_document_root(&root), root.join("public"));
    std::fs::remove_dir_all(root).expect("remove static project");
  }

  #[test]
  fn creates_site_with_generated_domain() {
    let root = temp_project();
    let site = create_site(SiteInput {
      name: None,
      domain: None,
      project_path: root.clone(),
      document_root: None,
      php_version: Some("8.2".parse().expect("parse version")),
    })
    .expect("create site");
    assert!(site.domain.starts_with("fabdev-site-"));
    assert!(site.domain.ends_with(".test"));
    assert_eq!(
      site.document_root,
      root.canonicalize().expect("canonical root").join("public")
    );
    std::fs::remove_dir_all(root).expect("remove test project");
  }

  #[test]
  fn edits_site_identity_without_changing_runtime_state() {
    let root = temp_project();
    let previous = create_site(SiteInput {
      name: Some("Old name".to_owned()),
      domain: Some("old.test".to_owned()),
      project_path: root.clone(),
      document_root: None,
      php_version: Some("8.2".parse().expect("parse version")),
    })
    .expect("create Site");

    let updated = edit_site(
      &previous,
      SiteEditInput {
        name: "New name".to_owned(),
        domain: "NEW.test.".to_owned(),
        project_path: root.clone(),
        document_root: Some(root.join("public")),
      },
    )
    .expect("edit Site");

    assert_eq!(updated.id, previous.id);
    assert_eq!(updated.name, "New name");
    assert_eq!(updated.domain, "new.test");
    assert_eq!(updated.php_version, previous.php_version);
    assert_eq!(updated.enabled, previous.enabled);
    assert_eq!(updated.secured, previous.secured);
    assert!(matches!(
      edit_site(
        &previous,
        SiteEditInput {
          name: "   ".to_owned(),
          domain: "new.test".to_owned(),
          project_path: root.clone(),
          document_root: Some(root.join("public")),
        }
      ),
      Err(SiteError::EmptyName)
    ));
    std::fs::remove_dir_all(root).expect("remove test project");
  }

  #[test]
  fn rejects_domains_outside_test_tld() {
    assert!(normalize_domain("erp.local").is_err());
  }
}
