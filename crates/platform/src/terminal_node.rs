use std::path::{Path, PathBuf};

use crate::PlatformError;

#[cfg(windows)]
use crate::terminal_php::{
  broadcast_environment_change, path_contains, prepend_path_entry, remove_path_entry,
  set_windows_user_path, terminal_bin_path, windows_user_path,
};

const NODE_SHIM_NAMES: &[&str] = &["node.cmd", "npm.cmd", "npx.cmd", "corepack.cmd"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalNodeIntegrationState {
  pub enabled: bool,
  pub bin_path: PathBuf,
  pub shim_paths: Vec<PathBuf>,
}

pub fn terminal_node_state(
  data_root: impl AsRef<Path>,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  platform_terminal_node_state(data_root.as_ref())
}

pub fn enable_terminal_node(
  data_root: impl AsRef<Path>,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  platform_enable_terminal_node(data_root.as_ref())
}

pub fn disable_terminal_node(
  data_root: impl AsRef<Path>,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  platform_disable_terminal_node(data_root.as_ref())
}

fn node_shim_paths(bin_path: &Path) -> Vec<PathBuf> {
  NODE_SHIM_NAMES
    .iter()
    .map(|name| bin_path.join(name))
    .collect()
}

#[cfg(windows)]
fn platform_terminal_node_state(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  let shim_paths = node_shim_paths(&bin_path);
  let user_path = windows_user_path()?;
  Ok(TerminalNodeIntegrationState {
    enabled: shim_paths.iter().all(|path| path.is_file()) && path_contains(&user_path, &bin_path),
    bin_path,
    shim_paths,
  })
}

#[cfg(windows)]
fn platform_enable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  std::fs::create_dir_all(&bin_path)?;
  for (name, command) in [
    ("node.cmd", "node.exe"),
    ("npm.cmd", "npm.cmd"),
    ("npx.cmd", "npx.cmd"),
    ("corepack.cmd", "corepack.cmd"),
  ] {
    std::fs::write(bin_path.join(name), windows_node_shim(command))?;
  }
  let user_path = windows_user_path()?;
  if !path_contains(&user_path, &bin_path) {
    set_windows_user_path(&prepend_path_entry(&user_path, &bin_path))?;
    broadcast_environment_change();
  }
  platform_terminal_node_state(data_root)
}

#[cfg(windows)]
fn platform_disable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  for path in node_shim_paths(&bin_path) {
    match std::fs::remove_file(path) {
      Ok(()) => {}
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => return Err(error.into()),
    }
  }
  let user_path = windows_user_path()?;
  let php_shim = bin_path.join("php.cmd");
  if !php_shim.is_file() && path_contains(&user_path, &bin_path) {
    set_windows_user_path(&remove_path_entry(&user_path, &bin_path))?;
    broadcast_environment_change();
  }
  platform_terminal_node_state(data_root)
}

#[cfg(any(windows, test))]
fn windows_node_shim(command: &str) -> String {
  format!(
    r#"@echo off
setlocal EnableExtensions EnableDelayedExpansion
set "FABDEV_DATA_ROOT=%~dp0.."
set "FABDEV_VERSION_FILE=%FABDEV_DATA_ROOT%\runtimes\node\current.version"
if not exist "%FABDEV_VERSION_FILE%" exit /b 127
set /p FABDEV_NODE_VERSION=<"%FABDEV_VERSION_FILE%"
echo(!FABDEV_NODE_VERSION!| %SystemRoot%\System32\findstr.exe /r /x "[0-9][0-9.]*" >nul
if errorlevel 1 exit /b 127
set "FABDEV_NODE_COMMAND=%FABDEV_DATA_ROOT%\runtimes\node\!FABDEV_NODE_VERSION!\{command}"
if not exist "%FABDEV_NODE_COMMAND%" exit /b 127
call "%FABDEV_NODE_COMMAND%" %*
exit /b %ERRORLEVEL%
"#
  )
}

#[cfg(not(windows))]
fn platform_terminal_node_state(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = data_root.join("bin");
  Ok(TerminalNodeIntegrationState {
    enabled: false,
    shim_paths: node_shim_paths(&bin_path),
    bin_path,
  })
}

#[cfg(not(windows))]
fn platform_enable_terminal_node(
  _data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  Err(PlatformError::Unsupported)
}

#[cfg(not(windows))]
fn platform_disable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  platform_terminal_node_state(data_root)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn windows_node_shims_follow_the_selected_version() {
    for command in ["node.exe", "npm.cmd", "npx.cmd", "corepack.cmd"] {
      let shim = windows_node_shim(command);
      assert!(shim.contains(r"runtimes\node\current.version"));
      assert!(shim.contains(&format!(r"!FABDEV_NODE_VERSION!\{command}")));
    }
  }
}
