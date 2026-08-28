use std::path::Path;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemOperation {
  InstallTestResolver,
  RemoveOwnedTestResolver,
  BindHttpPort,
  ReloadNginx,
}

#[derive(Debug, Error)]
pub enum PlatformError {
  #[error("operation is not supported on this platform")]
  Unsupported,
  #[error("system helper rejected operation: {0}")]
  Rejected(String),
}

#[async_trait]
pub trait SystemIntegration: Send + Sync {
  async fn install_test_resolver(&self, nameserver: &str) -> Result<(), PlatformError>;
  async fn validate_nginx_config(&self, config: &Path) -> Result<(), PlatformError>;
  async fn reload_nginx(&self) -> Result<(), PlatformError>;
}
