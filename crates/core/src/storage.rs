use std::path::Path;
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use uuid::Uuid;

use crate::{PhpVersion, ProxyConnectionSettings, Site};

const NO_PHP_VERSION: &str = "-";
const LINKED_SITE_SOURCE: &str = "linked";
const HOME_SITE_SOURCE: &str = "home";
const SITE_HOME_SETTING_KEY: &str = "site_home";
const PROXY_CONNECTIONS_SETTING_KEY: &str = "proxy_connections";
const PROXY_RUNNING_IDS_SETTING_KEY: &str = "proxy_running_ids";

pub struct SiteRepository {
  connection: Connection,
}

#[derive(Debug, Error)]
pub enum StorageError {
  #[error("database error: {0}")]
  Database(#[from] rusqlite::Error),
  #[error("invalid site id in database: {0}")]
  InvalidId(#[from] uuid::Error),
  #[error("invalid PHP version in database: {0}")]
  InvalidPhpVersion(String),
  #[error("invalid Proxy connections in database: {0}")]
  InvalidProxyConnections(String),
}

impl SiteRepository {
  pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
    let connection = Connection::open(path)?;
    let repository = Self { connection };
    repository.migrate()?;
    Ok(repository)
  }

  pub fn in_memory() -> Result<Self, StorageError> {
    let connection = Connection::open_in_memory()?;
    let repository = Self { connection };
    repository.migrate()?;
    Ok(repository)
  }

  fn migrate(&self) -> Result<(), StorageError> {
    self.connection.execute_batch(
      "
      PRAGMA foreign_keys = ON;
      CREATE TABLE IF NOT EXISTS sites (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        domain TEXT NOT NULL UNIQUE,
        project_path TEXT NOT NULL,
        document_root TEXT NOT NULL,
        php_version TEXT NOT NULL,
        node_version TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        secured INTEGER NOT NULL DEFAULT 0,
        source TEXT NOT NULL DEFAULT 'linked'
      );
      CREATE TABLE IF NOT EXISTS app_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
      ",
    )?;
    let has_source = {
      let mut statement = self.connection.prepare("PRAGMA table_info(sites)")?;
      let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
      columns
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|column| column == "source")
    };
    if !has_source {
      self.connection.execute(
        "ALTER TABLE sites ADD COLUMN source TEXT NOT NULL DEFAULT 'linked'",
        [],
      )?;
    }
    let has_secured = {
      let mut statement = self.connection.prepare("PRAGMA table_info(sites)")?;
      let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
      columns
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|column| column == "secured")
    };
    if !has_secured {
      self.connection.execute(
        "ALTER TABLE sites ADD COLUMN secured INTEGER NOT NULL DEFAULT 0",
        [],
      )?;
    }
    let has_node_version = {
      let mut statement = self.connection.prepare("PRAGMA table_info(sites)")?;
      let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
      columns
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|column| column == "node_version")
    };
    if !has_node_version {
      self
        .connection
        .execute("ALTER TABLE sites ADD COLUMN node_version TEXT", [])?;
    }
    Ok(())
  }

  pub fn insert(&self, site: &Site) -> Result<(), StorageError> {
    self.insert_with_source(site, LINKED_SITE_SOURCE)
  }

  fn insert_with_source(&self, site: &Site, source: &str) -> Result<(), StorageError> {
    self.connection.execute(
      "INSERT INTO sites
         (id, name, domain, project_path, document_root, php_version, enabled, secured, source)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
      params![
        site.id.to_string(),
        site.name,
        site.domain,
        site.project_path.to_string_lossy(),
        site.document_root.to_string_lossy(),
        serialize_php_version(site.php_version.as_ref()),
        site.enabled,
        site.secured,
        source,
      ],
    )?;
    Ok(())
  }

  pub fn list(&self) -> Result<Vec<Site>, StorageError> {
    let mut statement = self.connection.prepare(
      "SELECT id, name, domain, project_path, document_root, php_version, enabled, secured
       FROM sites ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| {
      Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, bool>(6)?,
        row.get::<_, bool>(7)?,
      ))
    })?;
    rows
      .map(|row| {
        let (id, name, domain, project_path, document_root, php_version, enabled, secured) = row?;
        let php_version = parse_php_version(&php_version)?;
        Ok(Site {
          id: Uuid::parse_str(&id)?,
          name,
          domain,
          project_path: project_path.into(),
          document_root: document_root.into(),
          php_version,
          enabled,
          secured,
        })
      })
      .collect()
  }

  pub fn list_home_sites(&self) -> Result<Vec<Site>, StorageError> {
    let mut statement = self.connection.prepare(
      "SELECT id, name, domain, project_path, document_root, php_version, enabled, secured
       FROM sites WHERE source = ?1 ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([HOME_SITE_SOURCE], |row| {
      Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, bool>(6)?,
        row.get::<_, bool>(7)?,
      ))
    })?;
    rows
      .map(|row| {
        let (id, name, domain, project_path, document_root, php_version, enabled, secured) = row?;
        let php_version = parse_php_version(&php_version)?;
        Ok(Site {
          id: Uuid::parse_str(&id)?,
          name,
          domain,
          project_path: project_path.into(),
          document_root: document_root.into(),
          php_version,
          enabled,
          secured,
        })
      })
      .collect()
  }

  pub fn replace_home_sites(&mut self, sites: &[Site]) -> Result<(), StorageError> {
    let transaction = self.connection.transaction()?;
    transaction.execute("DELETE FROM sites WHERE source = ?1", [HOME_SITE_SOURCE])?;
    for site in sites {
      transaction.execute(
        "INSERT INTO sites
           (id, name, domain, project_path, document_root, php_version, enabled, secured, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
          site.id.to_string(),
          site.name,
          site.domain,
          site.project_path.to_string_lossy(),
          site.document_root.to_string_lossy(),
          serialize_php_version(site.php_version.as_ref()),
          site.enabled,
          site.secured,
          HOME_SITE_SOURCE,
        ],
      )?;
    }
    transaction.commit()?;
    Ok(())
  }

  pub fn is_home_site(&self, id: &Uuid) -> Result<bool, StorageError> {
    Ok(
      self
        .connection
        .query_row(
          "SELECT source FROM sites WHERE id = ?1",
          [id.to_string()],
          |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|source| source == HOME_SITE_SOURCE),
    )
  }

  pub fn site_home(&self) -> Result<Option<std::path::PathBuf>, StorageError> {
    Ok(
      self
        .connection
        .query_row(
          "SELECT value FROM app_settings WHERE key = ?1",
          [SITE_HOME_SETTING_KEY],
          |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(Into::into),
    )
  }

  pub fn save_site_home(&self, path: &Path) -> Result<(), StorageError> {
    self.connection.execute(
      "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      params![SITE_HOME_SETTING_KEY, path.to_string_lossy()],
    )?;
    Ok(())
  }

  pub fn proxy_running_ids(&self) -> Result<Vec<String>, StorageError> {
    let value = self
      .connection
      .query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [PROXY_RUNNING_IDS_SETTING_KEY],
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    Ok(
      value
        .into_iter()
        .flat_map(|value| {
          value
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
        })
        .collect(),
    )
  }

  pub fn proxy_connections(&self) -> Result<Option<Vec<ProxyConnectionSettings>>, StorageError> {
    let value = self
      .connection
      .query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [PROXY_CONNECTIONS_SETTING_KEY],
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    value
      .map(|value| {
        serde_json::from_str(&value)
          .map_err(|error| StorageError::InvalidProxyConnections(error.to_string()))
      })
      .transpose()
  }

  pub fn save_proxy_connections(
    &self,
    connections: &[ProxyConnectionSettings],
  ) -> Result<(), StorageError> {
    let mut connections = connections.to_vec();
    connections.sort_by_key(|connection| connection.listen_port);
    let value = serde_json::to_string(&connections)
      .map_err(|error| StorageError::InvalidProxyConnections(error.to_string()))?;
    self.connection.execute(
      "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      params![PROXY_CONNECTIONS_SETTING_KEY, value],
    )?;
    Ok(())
  }

  pub fn save_proxy_running_ids(&self, ids: &[String]) -> Result<(), StorageError> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    self.connection.execute(
      "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      params![PROXY_RUNNING_IDS_SETTING_KEY, ids.join(",")],
    )?;
    Ok(())
  }

  pub fn remove(&self, id: &Uuid) -> Result<Option<Site>, StorageError> {
    let site = self.list()?.into_iter().find(|site| &site.id == id);
    let Some(site) = site else {
      return Ok(None);
    };
    self.connection.execute(
      "DELETE FROM sites WHERE id = ?1",
      params![site.id.to_string()],
    )?;
    Ok(Some(site))
  }

  pub fn update_site(&self, site: &Site) -> Result<Option<(Site, Site)>, StorageError> {
    let Some(previous) = self
      .list()?
      .into_iter()
      .find(|existing| existing.id == site.id)
    else {
      return Ok(None);
    };
    self.connection.execute(
      "UPDATE sites
       SET name = ?1, domain = ?2, project_path = ?3, document_root = ?4
       WHERE id = ?5",
      params![
        site.name,
        site.domain,
        site.project_path.to_string_lossy(),
        site.document_root.to_string_lossy(),
        site.id.to_string(),
      ],
    )?;
    Ok(Some((previous, site.clone())))
  }

  pub fn update_php_version(
    &self,
    id: &Uuid,
    php_version: Option<&PhpVersion>,
  ) -> Result<Option<(Site, Site)>, StorageError> {
    let Some(previous) = self.list()?.into_iter().find(|site| &site.id == id) else {
      return Ok(None);
    };
    let mut updated = previous.clone();
    updated.php_version = php_version.cloned();
    self.connection.execute(
      "UPDATE sites SET php_version = ?1 WHERE id = ?2",
      params![serialize_php_version(php_version), id.to_string()],
    )?;
    Ok(Some((previous, updated)))
  }

  pub fn update_https(
    &self,
    id: &Uuid,
    secured: bool,
  ) -> Result<Option<(Site, Site)>, StorageError> {
    let Some(previous) = self.list()?.into_iter().find(|site| &site.id == id) else {
      return Ok(None);
    };
    let mut updated = previous.clone();
    updated.secured = secured;
    self.connection.execute(
      "UPDATE sites SET secured = ?1 WHERE id = ?2",
      params![secured, id.to_string()],
    )?;
    Ok(Some((previous, updated)))
  }
}

fn serialize_php_version(php_version: Option<&PhpVersion>) -> String {
  php_version
    .map(ToString::to_string)
    .unwrap_or_else(|| NO_PHP_VERSION.to_owned())
}

fn parse_php_version(value: &str) -> Result<Option<PhpVersion>, StorageError> {
  if value == NO_PHP_VERSION {
    return Ok(None);
  }
  PhpVersion::from_str(value)
    .map(Some)
    .map_err(|_| StorageError::InvalidPhpVersion(value.to_owned()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn persists_and_lists_sites() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/erp".into(),
      document_root: "/tmp/erp/public".into(),
      php_version: Some("8.2".parse().expect("parse version")),
      enabled: true,
      secured: false,
    };
    repository.insert(&site).expect("insert site");
    assert_eq!(repository.list().expect("list sites"), vec![site]);
  }

  #[test]
  fn removes_site_by_id_without_touching_other_sites() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let removed = Site {
      id: Uuid::new_v4(),
      name: "ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/erp".into(),
      document_root: "/tmp/erp/public".into(),
      php_version: Some("8.2".parse().expect("parse version")),
      enabled: true,
      secured: false,
    };
    let retained = Site {
      id: Uuid::new_v4(),
      name: "CRM".to_owned(),
      domain: "crm.test".to_owned(),
      project_path: "/tmp/crm".into(),
      document_root: "/tmp/crm/public".into(),
      php_version: Some("8.2".parse().expect("parse version")),
      enabled: true,
      secured: false,
    };
    repository.insert(&removed).expect("insert removed site");
    repository.insert(&retained).expect("insert retained site");

    assert_eq!(
      repository.remove(&removed.id).expect("remove site"),
      Some(removed)
    );
    assert_eq!(repository.list().expect("list sites"), vec![retained]);
    assert_eq!(
      repository
        .remove(&Uuid::new_v4())
        .expect("remove missing site"),
      None
    );
  }

  #[test]
  fn updates_site_name_domain_and_paths_without_changing_runtime_state() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "Old ERP".to_owned(),
      domain: "old-erp.test".to_owned(),
      project_path: "/tmp/old-erp".into(),
      document_root: "/tmp/old-erp/public".into(),
      php_version: Some("8.2".parse().expect("parse version")),
      enabled: true,
      secured: true,
    };
    repository.insert(&site).expect("insert Site");
    let mut updated = site.clone();
    updated.name = "New ERP".to_owned();
    updated.domain = "new-erp.test".to_owned();
    updated.project_path = "/tmp/new-erp".into();
    updated.document_root = "/tmp/new-erp/web".into();

    let (previous, persisted) = repository
      .update_site(&updated)
      .expect("update Site")
      .expect("find Site");

    assert_eq!(previous, site);
    assert_eq!(persisted, updated);
    assert_eq!(repository.list().expect("list Sites"), vec![updated]);
  }

  #[test]
  fn rejects_site_update_to_a_duplicate_domain() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = |name: &str, domain: &str| Site {
      id: Uuid::new_v4(),
      name: name.to_owned(),
      domain: domain.to_owned(),
      project_path: format!("/tmp/{name}").into(),
      document_root: format!("/tmp/{name}/public").into(),
      php_version: None,
      enabled: true,
      secured: false,
    };
    let first = site("first", "first.test");
    let second = site("second", "second.test");
    repository.insert(&first).expect("insert first Site");
    repository.insert(&second).expect("insert second Site");
    let mut duplicate = second.clone();
    duplicate.domain = first.domain.clone();

    assert!(repository.update_site(&duplicate).is_err());
    assert_eq!(repository.list().expect("list Sites"), vec![first, second]);
  }

  #[test]
  fn updates_only_the_site_php_version() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/erp".into(),
      document_root: "/tmp/erp/public".into(),
      php_version: Some("8.2".parse().expect("parse PHP 8.2")),
      enabled: true,
      secured: false,
    };
    repository.insert(&site).expect("insert Site");

    let (previous, updated) = repository
      .update_php_version(&site.id, Some(&"7.4".parse().expect("parse PHP 7.4")))
      .expect("update PHP")
      .expect("find Site");
    assert_eq!(
      previous.php_version.expect("previous PHP").to_string(),
      "8.2"
    );
    assert_eq!(
      updated
        .php_version
        .as_ref()
        .expect("updated PHP")
        .to_string(),
      "7.4"
    );
    assert_eq!(repository.list().expect("list Sites"), vec![updated]);
  }

  #[test]
  fn persists_site_https_setting() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "Secure ERP".to_owned(),
      domain: "secure-erp.test".to_owned(),
      project_path: "/tmp/secure-erp".into(),
      document_root: "/tmp/secure-erp/public".into(),
      php_version: Some("8.2".parse().expect("parse PHP 8.2")),
      enabled: true,
      secured: false,
    };
    repository.insert(&site).expect("insert Site");

    let (previous, updated) = repository
      .update_https(&site.id, true)
      .expect("update HTTPS")
      .expect("find Site");

    assert!(!previous.secured);
    assert!(updated.secured);
    assert_eq!(repository.list().expect("list Sites"), vec![updated]);
  }

  #[test]
  fn persists_site_without_php() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "Static".to_owned(),
      domain: "static.test".to_owned(),
      project_path: "/tmp/static".into(),
      document_root: "/tmp/static/public".into(),
      php_version: None,
      enabled: true,
      secured: false,
    };
    repository.insert(&site).expect("insert Site");
    assert_eq!(repository.list().expect("list Sites"), vec![site]);
  }

  #[test]
  fn ignores_legacy_site_node_version_values() {
    let repository = SiteRepository::in_memory().expect("create repository");
    let site = Site {
      id: Uuid::new_v4(),
      name: "Legacy frontend".to_owned(),
      domain: "legacy-frontend.test".to_owned(),
      project_path: "/tmp/legacy-frontend".into(),
      document_root: "/tmp/legacy-frontend/public".into(),
      php_version: None,
      enabled: true,
      secured: false,
    };
    repository.insert(&site).expect("insert Site");
    repository
      .connection
      .execute(
        "UPDATE sites SET node_version = ?1 WHERE id = ?2",
        params!["24.19.0", site.id.to_string()],
      )
      .expect("seed legacy Node.js selection");

    assert_eq!(repository.list().expect("list Sites"), vec![site]);
  }

  #[test]
  fn replaces_home_sites_without_touching_linked_sites() {
    let mut repository = SiteRepository::in_memory().expect("create repository");
    let linked = Site {
      id: Uuid::new_v4(),
      name: "Linked ERP".to_owned(),
      domain: "erp.test".to_owned(),
      project_path: "/tmp/linked-erp".into(),
      document_root: "/tmp/linked-erp/public".into(),
      php_version: Some("8.2".parse().expect("parse PHP")),
      enabled: true,
      secured: false,
    };
    let home = Site {
      id: Uuid::new_v4(),
      name: "Site One".to_owned(),
      domain: "site1.test".to_owned(),
      project_path: "/tmp/Sites/site1".into(),
      document_root: "/tmp/Sites/site1/public".into(),
      php_version: Some("8.2".parse().expect("parse PHP")),
      enabled: true,
      secured: false,
    };
    repository.insert(&linked).expect("insert linked Site");
    repository
      .replace_home_sites(std::slice::from_ref(&home))
      .expect("insert Home Site");
    repository
      .replace_home_sites(&[])
      .expect("remove Home Sites");

    assert_eq!(repository.list().expect("list Sites"), vec![linked]);
    assert!(!repository.is_home_site(&home.id).expect("check Home Site"));
  }

  #[test]
  fn persists_site_home_path() {
    let repository = SiteRepository::in_memory().expect("create repository");
    assert_eq!(repository.site_home().expect("load empty Home path"), None);

    repository
      .save_site_home(Path::new("/Users/dev/Sites"))
      .expect("save Home path");

    assert_eq!(
      repository.site_home().expect("load Home path"),
      Some("/Users/dev/Sites".into())
    );
  }

  #[test]
  fn persists_sorted_proxy_running_ids() {
    let repository = SiteRepository::in_memory().expect("create repository");
    assert!(repository
      .proxy_running_ids()
      .expect("load empty Proxy state")
      .is_empty());

    repository
      .save_proxy_running_ids(&["alpha".to_owned(), "beta".to_owned(), "alpha".to_owned()])
      .expect("save Proxy state");

    assert_eq!(
      repository.proxy_running_ids().expect("load Proxy state"),
      vec!["alpha".to_owned(), "beta".to_owned()]
    );
  }

  #[test]
  fn persists_proxy_connections_sorted_by_port() {
    let repository = SiteRepository::in_memory().expect("create repository");
    assert_eq!(
      repository
        .proxy_connections()
        .expect("load empty Proxy connections"),
      None
    );
    let connection = |id: &str, port: u16| ProxyConnectionSettings {
      id: id.to_owned(),
      name: id.to_ascii_uppercase(),
      domain: format!("{id}.test"),
      listen_host: "127.0.0.1".to_owned(),
      listen_port: port,
      target: "http://127.0.0.1:9".to_owned(),
      allowed_origins: vec![format!("http://{id}.test")],
    };
    let expected = vec![connection("first", 3020), connection("second", 3021)];

    repository
      .save_proxy_connections(&[expected[1].clone(), expected[0].clone()])
      .expect("save Proxy connections");

    assert_eq!(
      repository
        .proxy_connections()
        .expect("load Proxy connections"),
      Some(expected)
    );
  }
}
