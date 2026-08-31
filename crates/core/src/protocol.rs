use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Site, SiteEditInput, SiteInput};

pub const PROTOCOL_VERSION: u16 = 36;
pub const SUPPORTED_NODE_VERSIONS: &[&str] = &["20.20.2", "24.20.0"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
  tag = "type",
  content = "payload",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum AgentRequest {
  Ping,
  GetStatus,
  ListSites,
  GetSiteHome,
  SaveSiteHome(SiteHomeInput),
  AddSite(SiteInput),
  UpdateSite {
    site_id: uuid::Uuid,
    input: SiteEditInput,
  },
  RemoveSite {
    site_id: uuid::Uuid,
  },
  SetSitePhp {
    site_id: uuid::Uuid,
    php_version: Option<crate::PhpVersion>,
  },
  EnsureLocalCa,
  SetSiteHttps {
    site_id: uuid::Uuid,
    secured: bool,
  },
  GetLanShare,
  StartLanShare {
    site_id: uuid::Uuid,
    port: u16,
  },
  StopLanShareSite {
    site_id: uuid::Uuid,
  },
  StopLanShare,
  CheckRuntimeUpdates,
  StartRuntimeDownload {
    name: String,
    version: String,
  },
  GetRuntimeUpdateOperation {
    operation_id: uuid::Uuid,
  },
  CancelRuntimeDownload {
    operation_id: uuid::Uuid,
  },
  InstallDownloadedRuntime {
    operation_id: uuid::Uuid,
  },
  ListPhpRuntimes,
  InstallPhpRuntime {
    artifact_path: PathBuf,
    release_path: PathBuf,
  },
  SetGlobalPhp {
    version: String,
  },
  GetTerminalPhp,
  EnableTerminalPhp,
  DisableTerminalPhp,
  RemovePhpRuntime {
    version: String,
  },
  GetPhpIni {
    php_version: crate::PhpVersion,
  },
  SavePhpIni {
    php_version: crate::PhpVersion,
    contents: String,
  },
  GetDefaultPhpIni,
  SaveDefaultPhpIni {
    contents: String,
  },
  GetErpPhpIni {
    php_version: Option<crate::PhpVersion>,
  },
  GetNodeRuntime,
  InstallNodeRuntime {
    artifact_path: PathBuf,
    release_path: PathBuf,
  },
  SetGlobalNode {
    version: String,
  },
  EnableTerminalNode,
  DisableTerminalNode,
  RemoveNodeRuntime {
    version: String,
  },
  GetProxyManager,
  AddProxyConnection(ProxyConnectionInput),
  UpdateProxyConnection {
    connection_id: String,
    input: ProxyConnectionInput,
  },
  RemoveProxyConnection {
    connection_id: String,
  },
  StartProxyConnection {
    connection_id: String,
  },
  StopProxyConnection {
    connection_id: String,
  },
  StartAllProxyConnections,
  StopAllProxyConnections,
  Shutdown,
  StartAll,
  StopAll,
  StartMariaDb,
  StopMariaDb,
  RestoreMariaDbLastState,
  GetMariaDbSettings,
  SaveMariaDbSettings(MariaDbSettings),
  GetMariaDbConfig,
  SaveMariaDbConfig {
    contents: String,
  },
  SetMariaDbRootPassword {
    current_password: String,
    new_password: String,
  },
  InstallMariaDbRuntime {
    artifact_path: PathBuf,
    release_path: PathBuf,
  },
  RemoveMariaDbRuntime,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
  tag = "type",
  content = "payload",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum AgentResponse {
  Pong {
    protocol_version: u16,
  },
  Status(AgentStatus),
  Sites(Vec<Site>),
  SiteHomeSettings(SiteHomeSettings),
  SiteAdded(Site),
  SiteUpdated(Site),
  SiteRemoved(Site),
  SitePhpChanged(Site),
  LocalCaReady(LocalCaInfo),
  SiteHttpsChanged(Site),
  LanShare(Option<LanShareInfo>),
  RuntimeUpdates(RuntimeUpdateCheck),
  RuntimeUpdateOperation(RuntimeUpdateOperation),
  PhpRuntimes(PhpRuntimeState),
  PhpRuntimeInstalled(PhpRuntimeState),
  GlobalPhpChanged(PhpRuntimeState),
  TerminalPhp(TerminalPhpState),
  PhpRuntimeRemoved(PhpRuntimeState),
  PhpIni {
    php_version: crate::PhpVersion,
    contents: String,
  },
  PhpIniSaved {
    php_version: crate::PhpVersion,
  },
  DefaultPhpIni {
    contents: String,
  },
  DefaultPhpIniSaved,
  ErpPhpIni {
    php_version: Option<crate::PhpVersion>,
    contents: String,
  },
  NodeRuntime(NodeRuntimeState),
  NodeRuntimeInstalled(NodeRuntimeState),
  GlobalNodeChanged(NodeRuntimeState),
  TerminalNode(NodeRuntimeState),
  NodeRuntimeRemoved(NodeRuntimeState),
  ProxyManager(ProxyManagerState),
  Started,
  Stopped,
  MariaDbStarted,
  MariaDbStopped,
  MariaDbStateRestored,
  MariaDbSettings(MariaDbSettings),
  MariaDbConfig {
    filename: String,
    contents: String,
  },
  MariaDbConfigSaved {
    filename: String,
    contents: String,
  },
  MariaDbRootPasswordChanged,
  MariaDbRuntimeInstalled {
    version: String,
  },
  MariaDbRuntimeRemoved {
    version: String,
  },
  Error {
    code: String,
    message: String,
  },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MariaDbSettings {
  pub port: u16,
  pub data_dir: PathBuf,
  #[serde(default)]
  pub connection_mode: MariaDbConnectionMode,
  #[serde(default = "default_mariadb_system_socket")]
  pub system_socket: PathBuf,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MariaDbConnectionMode {
  #[default]
  Managed,
  System,
}

pub fn default_mariadb_system_socket() -> PathBuf {
  #[cfg(unix)]
  {
    PathBuf::from("/tmp/mysql.sock")
  }
  #[cfg(windows)]
  {
    PathBuf::from(r"\\.\pipe\MySQL")
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCaInfo {
  pub certificate_path: PathBuf,
  pub fingerprint_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteHomeInput {
  pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteHomeSettings {
  pub path: PathBuf,
  pub site_ids: Vec<uuid::Uuid>,
  pub symbolic_link_site_ids: Vec<uuid::Uuid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareInfo {
  pub host: String,
  pub port: u16,
  pub sites: Vec<LanShareSiteInfo>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareSiteInfo {
  pub site_id: uuid::Uuid,
  pub domain: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpRuntimeState {
  pub global_version: Option<String>,
  pub installed: Vec<PhpRuntimeInfo>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpRuntimeInfo {
  pub version: String,
  pub series: String,
  pub active: bool,
  pub sites: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPhpState {
  pub enabled: bool,
  pub bin_path: PathBuf,
  pub shim_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRuntimeState {
  pub active_version: Option<String>,
  pub installed: Vec<NodeRuntimeInfo>,
  pub terminal: TerminalNodeState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRuntimeInfo {
  pub version: String,
  pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalNodeState {
  pub enabled: bool,
  pub bin_path: PathBuf,
  pub shim_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpdateCheck {
  pub catalog_sequence: u64,
  pub generated_at: String,
  pub expires_at: String,
  pub unsigned_community_build: bool,
  pub artifacts: Vec<RuntimeUpdateArtifact>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpdateArtifact {
  pub name: String,
  pub version: String,
  pub platform: String,
  pub architecture: String,
  pub minimum_os_version: String,
  pub file_name: String,
  pub size: u64,
  pub sha256: String,
  pub unsigned_community_build: bool,
  pub installed: bool,
  pub active_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpdateOperation {
  pub operation_id: uuid::Uuid,
  pub status: RuntimeUpdateOperationStatus,
  pub name: String,
  pub version: String,
  pub platform: String,
  pub architecture: String,
  pub file_name: String,
  pub bytes_downloaded: u64,
  pub total_bytes: u64,
  pub sha256: String,
  pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeUpdateOperationStatus {
  Queued,
  Downloading,
  Verified,
  Installing,
  Completed,
  Failed,
  Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConnectionSettings {
  pub id: String,
  pub name: String,
  pub domain: String,
  pub listen_host: String,
  pub listen_port: u16,
  pub target: String,
  pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConnectionInput {
  pub id: String,
  pub domain: String,
  pub listen_port: u16,
  pub target: String,
  pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyManagerState {
  pub connections: Vec<ProxyConnectionInfo>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConnectionInfo {
  pub id: String,
  pub name: String,
  pub domain: String,
  pub listen_host: String,
  pub listen_port: u16,
  pub target: String,
  pub allowed_origins: Vec<String>,
  pub state: ProxyConnectionState,
  pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyConnectionState {
  Starting,
  Running,
  Degraded,
  Stopping,
  Stopped,
  Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
  pub protocol_version: u16,
  pub agent_version: String,
  pub dns: ServiceState,
  pub nginx: ServiceState,
  pub php_fpm: ServiceState,
  #[serde(default)]
  pub php_fpm_pools: Vec<PhpFpmPoolStatus>,
  pub mariadb: ServiceState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpFpmPoolStatus {
  pub version: crate::PhpVersion,
  pub active_processes: u32,
  pub idle_processes: u32,
  pub total_processes: u32,
  pub listen_queue: u32,
  pub max_listen_queue: u32,
  pub max_children_reached: u64,
  pub slow_requests: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
  NotInstalled,
  Installed,
  Starting,
  Running,
  Stopping,
  Stopped,
  Updating,
  Failed,
}

impl AgentStatus {
  pub fn development() -> Self {
    Self {
      protocol_version: PROTOCOL_VERSION,
      agent_version: env!("CARGO_PKG_VERSION").to_owned(),
      dns: ServiceState::NotInstalled,
      nginx: ServiceState::NotInstalled,
      php_fpm: ServiceState::NotInstalled,
      php_fpm_pools: Vec::new(),
      mariadb: ServiceState::NotInstalled,
    }
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;
  use uuid::Uuid;

  use super::*;

  #[test]
  fn serializes_terminal_php_integration_contract() {
    assert_eq!(
      serde_json::to_value(AgentRequest::EnableTerminalPhp).expect("serialize request"),
      json!({ "type": "enableTerminalPhp" })
    );
    assert_eq!(
      serde_json::to_value(AgentResponse::TerminalPhp(TerminalPhpState {
        enabled: true,
        bin_path: PathBuf::from("/tmp/fabDev/bin"),
        shim_path: PathBuf::from("/tmp/fabDev/bin/php"),
      }))
      .expect("serialize response"),
      json!({
        "type": "terminalPhp",
        "payload": {
          "enabled": true,
          "binPath": "/tmp/fabDev/bin",
          "shimPath": "/tmp/fabDev/bin/php"
        }
      })
    );
  }

  #[test]
  fn serializes_remove_site_with_camel_case_fields() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    let value =
      serde_json::to_value(AgentRequest::RemoveSite { site_id }).expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "removeSite",
        "payload": { "siteId": "fabde000-0000-4000-8000-000000000001" }
      })
    );
  }

  #[test]
  fn serializes_site_update_with_camel_case_fields() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    let value = serde_json::to_value(AgentRequest::UpdateSite {
      site_id,
      input: SiteEditInput {
        name: "ERP Demo".to_owned(),
        domain: "erp-demo.test".to_owned(),
        project_path: "/Users/dev/erp-demo".into(),
        document_root: Some("/Users/dev/erp-demo/public".into()),
      },
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "updateSite",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001",
          "input": {
            "name": "ERP Demo",
            "domain": "erp-demo.test",
            "projectPath": "/Users/dev/erp-demo",
            "documentRoot": "/Users/dev/erp-demo/public"
          }
        }
      })
    );
  }

  #[test]
  fn serializes_default_php_ini_requests() {
    assert_eq!(
      serde_json::to_value(AgentRequest::GetDefaultPhpIni)
        .expect("serialize default php.ini request"),
      json!({ "type": "getDefaultPhpIni" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::SaveDefaultPhpIni {
        contents: "[PHP]\nmemory_limit = 256M\n".to_owned(),
      })
      .expect("serialize default php.ini save request"),
      json!({
        "type": "saveDefaultPhpIni",
        "payload": { "contents": "[PHP]\nmemory_limit = 256M\n" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::GetErpPhpIni {
        php_version: Some("7.4".parse().expect("parse PHP version")),
      })
      .expect("serialize ERP php.ini request"),
      json!({
        "type": "getErpPhpIni",
        "payload": { "phpVersion": "7.4" }
      })
    );
  }

  #[test]
  fn serializes_proxy_connection_management_with_camel_case_fields() {
    let add = AgentRequest::AddProxyConnection(ProxyConnectionInput {
      id: "custom".to_owned(),
      domain: "custom.test".to_owned(),
      listen_port: 3020,
      target: "http://api.example.test".to_owned(),
      allowed_origins: vec!["http://custom.test:8100".to_owned()],
    });
    assert_eq!(
      serde_json::to_value(add).expect("serialize add Proxy request"),
      json!({
        "type": "addProxyConnection",
        "payload": {
          "id": "custom",
          "domain": "custom.test",
          "listenPort": 3020,
          "target": "http://api.example.test",
          "allowedOrigins": ["http://custom.test:8100"]
        }
      })
    );

    assert_eq!(
      serde_json::to_value(AgentRequest::UpdateProxyConnection {
        connection_id: "custom".to_owned(),
        input: ProxyConnectionInput {
          id: "custom".to_owned(),
          domain: "custom.test".to_owned(),
          listen_port: 3021,
          target: "http://api.changed.test".to_owned(),
          allowed_origins: vec!["http://custom.test:8100".to_owned()],
        },
      })
      .expect("serialize update Proxy request"),
      json!({
        "type": "updateProxyConnection",
        "payload": {
          "connectionId": "custom",
          "input": {
            "id": "custom",
            "domain": "custom.test",
            "listenPort": 3021,
            "target": "http://api.changed.test",
            "allowedOrigins": ["http://custom.test:8100"]
          }
        }
      })
    );

    assert_eq!(
      serde_json::to_value(AgentRequest::RemoveProxyConnection {
        connection_id: "custom".to_owned(),
      })
      .expect("serialize remove Proxy request"),
      json!({
        "type": "removeProxyConnection",
        "payload": { "connectionId": "custom" }
      })
    );
  }

  #[test]
  fn serializes_site_home_settings() {
    assert_eq!(
      serde_json::to_value(AgentRequest::SaveSiteHome(SiteHomeInput {
        path: "/Users/dev/Sites".into(),
      }))
      .expect("serialize Site Home settings"),
      json!({
        "type": "saveSiteHome",
        "payload": { "path": "/Users/dev/Sites" }
      })
    );

    let home_site_id =
      uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("parse Home Site ID");
    let symbolic_link_site_id = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222")
      .expect("parse symbolic link Site ID");
    assert_eq!(
      serde_json::to_value(AgentResponse::SiteHomeSettings(SiteHomeSettings {
        path: "/Users/dev/Sites".into(),
        site_ids: vec![home_site_id, symbolic_link_site_id],
        symbolic_link_site_ids: vec![symbolic_link_site_id],
      }))
      .expect("serialize Site Home response"),
      json!({
        "type": "siteHomeSettings",
        "payload": {
          "path": "/Users/dev/Sites",
          "siteIds": [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222"
          ],
          "symbolicLinkSiteIds": ["22222222-2222-2222-2222-222222222222"]
        }
      })
    );
  }

  #[test]
  fn serializes_runtime_install_paths_with_camel_case_fields() {
    let value = serde_json::to_value(AgentRequest::InstallPhpRuntime {
      artifact_path: "/tmp/php.tar.gz".into(),
      release_path: "/tmp/php.json".into(),
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "installPhpRuntime",
        "payload": {
          "artifactPath": "/tmp/php.tar.gz",
          "releasePath": "/tmp/php.json"
        }
      })
    );
  }

  #[test]
  fn serializes_site_php_switch_with_camel_case_fields() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    let value = serde_json::to_value(AgentRequest::SetSitePhp {
      site_id,
      php_version: Some("7.4".parse().expect("parse PHP 7.4")),
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "setSitePhp",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001",
          "phpVersion": "7.4"
        }
      })
    );
  }

  #[test]
  fn serializes_disabled_site_php_as_null() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    let value = serde_json::to_value(AgentRequest::SetSitePhp {
      site_id,
      php_version: None,
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "setSitePhp",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001",
          "phpVersion": null
        }
      })
    );
  }

  #[test]
  fn serializes_node_runtime_install_paths() {
    let value = serde_json::to_value(AgentRequest::InstallNodeRuntime {
      artifact_path: "/tmp/node.tar.gz".into(),
      release_path: "/tmp/node.json".into(),
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "installNodeRuntime",
        "payload": {
          "artifactPath": "/tmp/node.tar.gz",
          "releasePath": "/tmp/node.json"
        }
      })
    );

    assert_eq!(
      serde_json::to_value(AgentRequest::SetGlobalNode {
        version: "24.20.0".to_owned(),
      })
      .expect("serialize global Node.js switch"),
      json!({
        "type": "setGlobalNode",
        "payload": { "version": "24.20.0" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::RemoveNodeRuntime {
        version: "20.20.2".to_owned(),
      })
      .expect("serialize Node.js removal"),
      json!({
        "type": "removeNodeRuntime",
        "payload": { "version": "20.20.2" }
      })
    );
  }

  #[test]
  fn serializes_runtime_update_protocol_with_camel_case_fields() {
    let operation_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000033").expect("parse operation ID");
    assert_eq!(
      serde_json::to_value(AgentRequest::CheckRuntimeUpdates)
        .expect("serialize Runtime update check"),
      json!({ "type": "checkRuntimeUpdates" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StartRuntimeDownload {
        name: "php".to_owned(),
        version: "8.4.24".to_owned(),
      })
      .expect("serialize Runtime download start"),
      json!({
        "type": "startRuntimeDownload",
        "payload": { "name": "php", "version": "8.4.24" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::CancelRuntimeDownload { operation_id })
        .expect("serialize Runtime download cancellation"),
      json!({
        "type": "cancelRuntimeDownload",
        "payload": { "operationId": "fabde000-0000-4000-8000-000000000033" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::GetRuntimeUpdateOperation { operation_id })
        .expect("serialize Runtime operation lookup"),
      json!({
        "type": "getRuntimeUpdateOperation",
        "payload": { "operationId": "fabde000-0000-4000-8000-000000000033" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::InstallDownloadedRuntime { operation_id })
        .expect("serialize Runtime installation request"),
      json!({
        "type": "installDownloadedRuntime",
        "payload": { "operationId": "fabde000-0000-4000-8000-000000000033" }
      })
    );

    let check = AgentResponse::RuntimeUpdates(RuntimeUpdateCheck {
      catalog_sequence: 3,
      generated_at: "2026-08-30T00:00:00Z".to_owned(),
      expires_at: "2027-02-26T00:00:00Z".to_owned(),
      unsigned_community_build: true,
      artifacts: vec![RuntimeUpdateArtifact {
        name: "node".to_owned(),
        version: "24.19.0".to_owned(),
        platform: "windows".to_owned(),
        architecture: "x64".to_owned(),
        minimum_os_version: "11.0".to_owned(),
        file_name: "node-24.19.0-windows-x64-community.tar.gz".to_owned(),
        size: 100,
        sha256: "a".repeat(64),
        unsigned_community_build: true,
        installed: false,
        active_version: Some("24.18.0".to_owned()),
      }],
    });
    assert_eq!(
      serde_json::to_value(check).expect("serialize Runtime update check"),
      json!({
        "type": "runtimeUpdates",
        "payload": {
          "catalogSequence": 3,
          "generatedAt": "2026-08-30T00:00:00Z",
          "expiresAt": "2027-02-26T00:00:00Z",
          "unsignedCommunityBuild": true,
          "artifacts": [{
            "name": "node",
            "version": "24.19.0",
            "platform": "windows",
            "architecture": "x64",
            "minimumOsVersion": "11.0",
            "fileName": "node-24.19.0-windows-x64-community.tar.gz",
            "size": 100,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "unsignedCommunityBuild": true,
            "installed": false,
            "activeVersion": "24.18.0"
          }]
        }
      })
    );

    let response = AgentResponse::RuntimeUpdateOperation(RuntimeUpdateOperation {
      operation_id,
      status: RuntimeUpdateOperationStatus::Downloading,
      name: "php".to_owned(),
      version: "8.4.24".to_owned(),
      platform: "macos".to_owned(),
      architecture: "arm64".to_owned(),
      file_name: "php-8.4.24-macos-arm64-community.tar.gz".to_owned(),
      bytes_downloaded: 50,
      total_bytes: 100,
      sha256: "a".repeat(64),
      error: None,
    });
    assert_eq!(
      serde_json::to_value(response).expect("serialize Runtime operation"),
      json!({
        "type": "runtimeUpdateOperation",
        "payload": {
          "operationId": "fabde000-0000-4000-8000-000000000033",
          "status": "downloading",
          "name": "php",
          "version": "8.4.24",
          "platform": "macos",
          "architecture": "arm64",
          "fileName": "php-8.4.24-macos-arm64-community.tar.gz",
          "bytesDownloaded": 50,
          "totalBytes": 100,
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "error": null
        }
      })
    );
  }

  #[test]
  fn serializes_proxy_manager_controls() {
    assert_eq!(
      serde_json::to_value(AgentRequest::GetProxyManager).expect("serialize Proxy Manager status"),
      json!({ "type": "getProxyManager" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StartProxyConnection {
        connection_id: "example".to_owned(),
      })
      .expect("serialize Proxy connection start"),
      json!({
        "type": "startProxyConnection",
        "payload": { "connectionId": "example" }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StopAllProxyConnections)
        .expect("serialize all Proxy connections stop"),
      json!({ "type": "stopAllProxyConnections" })
    );
  }

  #[test]
  fn serializes_site_https_switch_with_camel_case_fields() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    let value = serde_json::to_value(AgentRequest::SetSiteHttps {
      site_id,
      secured: true,
    })
    .expect("serialize request");

    assert_eq!(
      value,
      json!({
        "type": "setSiteHttps",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001",
          "secured": true
        }
      })
    );
  }

  #[test]
  fn serializes_lan_share_controls() {
    let site_id =
      Uuid::parse_str("fabde000-0000-4000-8000-000000000001").expect("parse fixture id");
    assert_eq!(
      serde_json::to_value(AgentRequest::StartLanShare {
        site_id,
        port: 18080,
      })
      .expect("serialize LAN Share start"),
      json!({
        "type": "startLanShare",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001",
          "port": 18080
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::GetLanShare).expect("serialize LAN Share status"),
      json!({ "type": "getLanShare" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StopLanShareSite { site_id })
        .expect("serialize LAN Share Site stop"),
      json!({
        "type": "stopLanShareSite",
        "payload": {
          "siteId": "fabde000-0000-4000-8000-000000000001"
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StopLanShare).expect("serialize LAN Share stop"),
      json!({ "type": "stopLanShare" })
    );
  }

  #[test]
  fn serializes_independent_mariadb_controls() {
    assert_eq!(
      serde_json::to_value(AgentRequest::StartMariaDb).expect("serialize MariaDB start"),
      json!({ "type": "startMariaDb" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::StopMariaDb).expect("serialize MariaDB stop"),
      json!({ "type": "stopMariaDb" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::RestoreMariaDbLastState)
        .expect("serialize MariaDB state restoration"),
      json!({ "type": "restoreMariaDbLastState" })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::SaveMariaDbSettings(MariaDbSettings {
        port: 3307,
        data_dir: "/tmp/fabDev MariaDB".into(),
        connection_mode: MariaDbConnectionMode::System,
        system_socket: "/tmp/mysql.sock".into(),
      }))
      .expect("serialize MariaDB settings"),
      json!({
        "type": "saveMariaDbSettings",
        "payload": {
          "port": 3307,
          "dataDir": "/tmp/fabDev MariaDB",
          "connectionMode": "system",
          "systemSocket": "/tmp/mysql.sock"
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::SaveMariaDbConfig {
        contents: "[mariadbd]\nmax_connections = 250\n".to_owned(),
      })
      .expect("serialize MariaDB configuration"),
      json!({
        "type": "saveMariaDbConfig",
        "payload": {
          "contents": "[mariadbd]\nmax_connections = 250\n"
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::SetMariaDbRootPassword {
        current_password: "old".to_owned(),
        new_password: "new".to_owned(),
      })
      .expect("serialize MariaDB root password change"),
      json!({
        "type": "setMariaDbRootPassword",
        "payload": {
          "currentPassword": "old",
          "newPassword": "new"
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::InstallMariaDbRuntime {
        artifact_path: "/tmp/mariadb.tar.gz".into(),
        release_path: "/tmp/mariadb.json".into(),
      })
      .expect("serialize MariaDB Runtime install"),
      json!({
        "type": "installMariaDbRuntime",
        "payload": {
          "artifactPath": "/tmp/mariadb.tar.gz",
          "releasePath": "/tmp/mariadb.json"
        }
      })
    );
    assert_eq!(
      serde_json::to_value(AgentRequest::RemoveMariaDbRuntime)
        .expect("serialize MariaDB Runtime removal"),
      json!({ "type": "removeMariaDbRuntime" })
    );
  }

  #[test]
  fn defaults_existing_mariadb_settings_to_managed_connection() {
    let settings: MariaDbSettings = serde_json::from_value(json!({
      "port": 3306,
      "dataDir": "/tmp/fabDev MariaDB"
    }))
    .expect("read settings saved before connection source support");

    assert_eq!(settings.connection_mode, MariaDbConnectionMode::Managed);
    assert_eq!(settings.system_socket, default_mariadb_system_socket());
  }
}
