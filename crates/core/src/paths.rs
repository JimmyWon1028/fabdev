use std::ffi::OsString;
use std::path::{Path, PathBuf};

use directories::{BaseDirs, ProjectDirs};

pub const WINDOWS_AGENT_PIPE: &str = concat!(r"\\.\pipe\Fab", "DevAgent-v1");
const LEGACY_DATA_DIRECTORY_NAME: &str = concat!("Fab", "Dev");

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEndpoint {
  #[cfg(unix)]
  UnixSocket(PathBuf),
  #[cfg(windows)]
  NamedPipe(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
  pub root: PathBuf,
  pub config: PathBuf,
  pub runtimes: PathBuf,
  pub services: PathBuf,
  pub sites: PathBuf,
  pub logs: PathBuf,
  pub cache: PathBuf,
  pub state: PathBuf,
}

impl AppPaths {
  pub fn discover() -> Option<Self> {
    let override_root = std::env::var_os("FABDEV_DATA_DIR");
    let default_root = ProjectDirs::from("", "", LEGACY_DATA_DIRECTORY_NAME)
      .map(|project_dirs| project_dirs.data_local_dir().to_path_buf());
    let root = select_data_root(override_root, default_root)?;
    Some(Self::from_root(root))
  }

  pub fn from_root(root: impl AsRef<Path>) -> Self {
    let root = root.as_ref().to_path_buf();
    Self {
      config: root.join("config"),
      runtimes: root.join("runtimes"),
      services: root.join("services"),
      sites: root.join("sites"),
      logs: root.join("logs"),
      cache: root.join("cache"),
      state: root.join("state"),
      root,
    }
  }

  pub fn ensure(&self) -> std::io::Result<()> {
    for path in [
      &self.root,
      &self.config,
      &self.runtimes,
      &self.services,
      &self.sites,
      &self.logs,
      &self.cache,
      &self.state,
    ] {
      std::fs::create_dir_all(path)?;
    }
    Ok(())
  }

  pub fn agent_endpoint(&self) -> AgentEndpoint {
    #[cfg(unix)]
    {
      AgentEndpoint::UnixSocket(self.state.join("agent.sock"))
    }
    #[cfg(windows)]
    {
      AgentEndpoint::NamedPipe(WINDOWS_AGENT_PIPE.to_owned())
    }
  }

  #[cfg(unix)]
  pub fn agent_socket(&self) -> PathBuf {
    match self.agent_endpoint() {
      AgentEndpoint::UnixSocket(path) => path,
    }
  }

  pub fn database(&self) -> PathBuf {
    self.state.join("fabdev.sqlite3")
  }
}

pub fn default_site_home() -> Option<PathBuf> {
  BaseDirs::new().map(|directories| directories.home_dir().join("Sites"))
}

fn select_data_root(
  override_root: Option<OsString>,
  default_root: Option<PathBuf>,
) -> Option<PathBuf> {
  match override_root {
    Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
    _ => default_root,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_predictable_subdirectories() {
    let paths = AppPaths::from_root("/tmp/fabdev-test");
    assert_eq!(paths.runtimes, PathBuf::from("/tmp/fabdev-test/runtimes"));
    #[cfg(unix)]
    assert_eq!(
      paths.agent_endpoint(),
      AgentEndpoint::UnixSocket(PathBuf::from("/tmp/fabdev-test/state/agent.sock"))
    );
  }

  #[test]
  fn prefers_non_empty_environment_override() {
    assert_eq!(
      select_data_root(
        Some(OsString::from("/tmp/fabdev-override")),
        Some(PathBuf::from("/tmp/fabdev-default")),
      ),
      Some(PathBuf::from("/tmp/fabdev-override"))
    );
    assert_eq!(
      select_data_root(
        Some(OsString::new()),
        Some(PathBuf::from("/tmp/fabdev-default")),
      ),
      Some(PathBuf::from("/tmp/fabdev-default"))
    );
  }
}
