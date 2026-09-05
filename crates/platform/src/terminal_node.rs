use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use directories::BaseDirs;

use crate::PlatformError;

#[cfg(windows)]
use crate::terminal_php::{
  broadcast_environment_change, path_contains, prepend_path_entry, remove_path_entry,
  set_windows_user_path, terminal_bin_path, windows_user_path,
};

#[cfg(target_os = "macos")]
const MACOS_SHELL_BLOCK_START: &str = "# >>> fabDev terminal Node.js >>>";
#[cfg(target_os = "macos")]
const MACOS_SHELL_BLOCK_END: &str = "# <<< fabDev terminal Node.js <<<";
#[cfg(target_os = "macos")]
const MACOS_NODE_SHIM_NAMES: &[&str] = &["node", "npm", "npx", "corepack"];
#[cfg(windows)]
const WINDOWS_NODE_SHIM_NAMES: &[&str] = &["node.cmd", "npm.cmd", "npx.cmd", "corepack.cmd"];

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

#[cfg(any(windows, target_os = "macos"))]
fn node_shim_paths(bin_path: &Path) -> Vec<PathBuf> {
  platform_node_shim_names()
    .iter()
    .map(|name| bin_path.join(name))
    .collect()
}

#[cfg(windows)]
fn platform_node_shim_names() -> &'static [&'static str] {
  WINDOWS_NODE_SHIM_NAMES
}

#[cfg(target_os = "macos")]
fn platform_node_shim_names() -> &'static [&'static str] {
  MACOS_NODE_SHIM_NAMES
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

#[cfg(target_os = "macos")]
fn platform_terminal_node_state(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  macos_terminal_node_state(data_root, &profile_path, &rc_path)
}

#[cfg(target_os = "macos")]
fn platform_enable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  enable_macos_terminal_node(data_root, &profile_path, &rc_path)
}

#[cfg(target_os = "macos")]
fn platform_disable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  disable_macos_terminal_node(data_root, &profile_path, &rc_path)
}

#[cfg(target_os = "macos")]
fn macos_shell_paths() -> Result<(PathBuf, PathBuf), PlatformError> {
  BaseDirs::new()
    .map(|directories| {
      (
        directories.home_dir().join(".zprofile"),
        directories.home_dir().join(".zshrc"),
      )
    })
    .ok_or_else(|| {
      PlatformError::InvalidTerminalIntegration(
        "unable to locate the current user's home directory".to_owned(),
      )
    })
}

#[cfg(target_os = "macos")]
fn macos_terminal_node_state(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = data_root.join("bin");
  let shim_paths = node_shim_paths(&bin_path);
  let profile = read_optional_text(profile_path)?;
  let rc = read_optional_text(rc_path)?;
  let shell_block = macos_shell_block(&bin_path)?;
  Ok(TerminalNodeIntegrationState {
    enabled: shim_paths.iter().all(|path| path.is_file())
      && profile.contains(&shell_block)
      && rc.contains(&shell_block),
    shim_paths,
    bin_path,
  })
}

#[cfg(target_os = "macos")]
fn enable_macos_terminal_node(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = data_root.join("bin");
  let existing_profile = read_optional_text(profile_path)?;
  let existing_rc = read_optional_text(rc_path)?;
  let shell_block = macos_shell_block(&bin_path)?;
  let updated_profile = append_macos_shell_block(&existing_profile, &shell_block)?;
  let updated_rc = append_macos_shell_block(&existing_rc, &shell_block)?;
  std::fs::create_dir_all(&bin_path)?;
  for command in MACOS_NODE_SHIM_NAMES {
    write_macos_node_shim(&bin_path.join(command), command)?;
  }
  atomic_write(rc_path, updated_rc.as_bytes())?;
  if let Err(error) = atomic_write(profile_path, updated_profile.as_bytes()) {
    let _ = atomic_write(rc_path, existing_rc.as_bytes());
    return Err(error);
  }
  macos_terminal_node_state(data_root, profile_path, rc_path)
}

#[cfg(target_os = "macos")]
fn disable_macos_terminal_node(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let existing_profile = read_optional_text(profile_path)?;
  let existing_rc = read_optional_text(rc_path)?;
  let updated_profile = remove_macos_shell_block(&existing_profile)?;
  let updated_rc = remove_macos_shell_block(&existing_rc)?;
  if updated_profile != existing_profile {
    atomic_write(profile_path, updated_profile.as_bytes())?;
  }
  if let Err(error) = if updated_rc != existing_rc {
    atomic_write(rc_path, updated_rc.as_bytes())
  } else {
    Ok(())
  } {
    if updated_profile != existing_profile {
      let _ = atomic_write(profile_path, existing_profile.as_bytes());
    }
    return Err(error);
  }
  for path in node_shim_paths(&data_root.join("bin")) {
    match std::fs::remove_file(path) {
      Ok(()) => {}
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => {
        if updated_profile != existing_profile {
          let _ = atomic_write(profile_path, existing_profile.as_bytes());
        }
        if updated_rc != existing_rc {
          let _ = atomic_write(rc_path, existing_rc.as_bytes());
        }
        return Err(error.into());
      }
    }
  }
  macos_terminal_node_state(data_root, profile_path, rc_path)
}

#[cfg(target_os = "macos")]
fn append_macos_shell_block(contents: &str, shell_block: &str) -> Result<String, PlatformError> {
  let mut updated = remove_macos_shell_block(contents)?;
  if !updated.is_empty() && !updated.ends_with('\n') {
    updated.push('\n');
  }
  if !updated.is_empty() && !updated.ends_with("\n\n") {
    updated.push('\n');
  }
  updated.push_str(shell_block);
  updated.push('\n');
  Ok(updated)
}

#[cfg(target_os = "macos")]
fn read_optional_text(path: &Path) -> Result<String, PlatformError> {
  match std::fs::read_to_string(path) {
    Ok(contents) => Ok(contents),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(target_os = "macos")]
fn macos_shell_block(bin_path: &Path) -> Result<String, PlatformError> {
  let bin_path = bin_path.to_str().ok_or_else(|| {
    PlatformError::InvalidTerminalIntegration("the terminal bin path is not valid UTF-8".to_owned())
  })?;
  if bin_path.contains('\n') || bin_path.contains('\r') {
    return Err(PlatformError::InvalidTerminalIntegration(
      "the terminal bin path contains a line break".to_owned(),
    ));
  }
  let escaped = bin_path.replace('\'', "'\"'\"'");
  Ok(format!(
    "{MACOS_SHELL_BLOCK_START}\nexport PATH='{escaped}':\"$PATH\"\n{MACOS_SHELL_BLOCK_END}"
  ))
}

#[cfg(target_os = "macos")]
fn write_macos_node_shim(path: &Path, command: &str) -> Result<(), PlatformError> {
  use std::os::unix::fs::PermissionsExt;

  if !MACOS_NODE_SHIM_NAMES.contains(&command) {
    return Err(PlatformError::InvalidTerminalIntegration(format!(
      "unsupported Node.js shim command: {command}"
    )));
  }
  let contents = format!(
    r#"#!/bin/sh
# Resolve the selected fabDev Node.js Runtime on every invocation.
FABDEV_BIN_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
FABDEV_DATA_ROOT=$(dirname -- "$FABDEV_BIN_DIR")
FABDEV_NODE_COMMAND="$FABDEV_DATA_ROOT/runtimes/node/current/bin/{command}"
if [ ! -x "$FABDEV_NODE_COMMAND" ]; then
  printf '%s\n' 'fabDev global Node.js is not installed or selected.' >&2
  exit 127
fi
exec "$FABDEV_NODE_COMMAND" "$@"
"#
  );
  atomic_write(path, contents.as_bytes())?;
  let mut permissions = std::fs::metadata(path)?.permissions();
  permissions.set_mode(0o755);
  std::fs::set_permissions(path, permissions)?;
  Ok(())
}

#[cfg(target_os = "macos")]
fn remove_macos_shell_block(contents: &str) -> Result<String, PlatformError> {
  let mut updated = contents.to_owned();
  loop {
    let Some(start) = updated.find(MACOS_SHELL_BLOCK_START) else {
      return Ok(updated);
    };
    let search_from = start + MACOS_SHELL_BLOCK_START.len();
    let Some(relative_end) = updated[search_from..].find(MACOS_SHELL_BLOCK_END) else {
      return Err(PlatformError::InvalidTerminalIntegration(
        "the shell profile contains an incomplete fabDev Node.js block".to_owned(),
      ));
    };
    let mut end = search_from + relative_end + MACOS_SHELL_BLOCK_END.len();
    if updated.as_bytes().get(end) == Some(&b'\r') {
      end += 1;
    }
    if updated.as_bytes().get(end) == Some(&b'\n') {
      end += 1;
    }
    let removal_start = if updated[..start].ends_with("\n\n") {
      start - 1
    } else if updated[..start].ends_with("\r\n\r\n") {
      start - 2
    } else {
      start
    };
    updated.replace_range(removal_start..end, "");
  }
}

#[cfg(target_os = "macos")]
fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), PlatformError> {
  let parent = path.parent().ok_or_else(|| {
    PlatformError::InvalidTerminalIntegration(format!(
      "{} does not have a parent directory",
      path.display()
    ))
  })?;
  std::fs::create_dir_all(parent)?;
  let file_name = path
    .file_name()
    .and_then(|name| name.to_str())
    .ok_or_else(|| {
      PlatformError::InvalidTerminalIntegration(format!(
        "{} does not have a valid file name",
        path.display()
      ))
    })?;
  let temporary = parent.join(format!(".{file_name}.fabdev-{}.tmp", std::process::id()));
  std::fs::write(&temporary, contents)?;
  if let Err(error) = std::fs::rename(&temporary, path) {
    let _ = std::fs::remove_file(&temporary);
    return Err(error.into());
  }
  Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_terminal_node_state(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  let bin_path = data_root.join("bin");
  Ok(TerminalNodeIntegrationState {
    enabled: false,
    shim_paths: Vec::new(),
    bin_path,
  })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_enable_terminal_node(
  _data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  Err(PlatformError::Unsupported)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_disable_terminal_node(
  data_root: &Path,
) -> Result<TerminalNodeIntegrationState, PlatformError> {
  platform_terminal_node_state(data_root)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[cfg(target_os = "macos")]
  fn rejects_invalid_node_profiles_before_writing_shims() {
    for (case, (profile, rc)) in [
      (MACOS_SHELL_BLOCK_START, "export EDITOR=vi\n"),
      ("export EDITOR=vi\n", MACOS_SHELL_BLOCK_START),
    ]
    .into_iter()
    .enumerate()
    {
      for existing in [false, true] {
        let root = std::env::temp_dir().join(format!(
          "fabdev-node-preflight-{}-{case}-{existing}",
          std::process::id()
        ));
        let data_root = root.join("data");
        let profile_path = root.join(".zprofile");
        let rc_path = root.join(".zshrc");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&profile_path, profile).unwrap();
        std::fs::write(&rc_path, rc).unwrap();
        let commands: &[&str] = MACOS_NODE_SHIM_NAMES;
        if existing {
          std::fs::create_dir_all(data_root.join("bin")).unwrap();
          for command in commands {
            std::fs::write(data_root.join("bin").join(command), "original shim\n").unwrap();
          }
        }
        let result = enable_macos_terminal_node(&data_root, &profile_path, &rc_path);
        let profiles_preserved = std::fs::read_to_string(&profile_path).unwrap() == profile
          && std::fs::read_to_string(&rc_path).unwrap() == rc;
        let shims_preserved = commands.iter().all(|command| {
          let path = data_root.join("bin").join(command);
          if existing {
            std::fs::read_to_string(path).unwrap() == "original shim\n"
          } else {
            !path.exists()
          }
        });
        std::fs::remove_dir_all(root).unwrap();
        assert!(result.is_err());
        assert!(profiles_preserved);
        assert!(
          shims_preserved,
          "invalid profile must not create or overwrite node shims"
        );
      }
    }
  }

  #[test]
  fn windows_node_shims_follow_the_selected_version() {
    for command in ["node.exe", "npm.cmd", "npx.cmd", "corepack.cmd"] {
      let shim = windows_node_shim(command);
      assert!(shim.contains(r"runtimes\node\current.version"));
      assert!(shim.contains(&format!(r"!FABDEV_NODE_VERSION!\{command}")));
    }
  }

  #[test]
  #[cfg(target_os = "macos")]
  fn enables_repairs_and_disables_macos_terminal_node() {
    let root =
      std::env::temp_dir().join(format!("fabdev-terminal-node-macos-{}", std::process::id()));
    let data_root = root.join("data");
    let profile_path = root.join("home/.zprofile");
    let rc_path = root.join("home/.zshrc");
    std::fs::create_dir_all(profile_path.parent().expect("profile parent"))
      .expect("create profile fixture");
    std::fs::write(&profile_path, "export PATH='/usr/bin'\n").expect("write profile fixture");
    std::fs::write(&rc_path, "export PATH='/opt/homebrew/bin':\"$PATH\"\n")
      .expect("write rc fixture");

    let enabled = enable_macos_terminal_node(&data_root, &profile_path, &rc_path)
      .expect("enable macOS terminal Node.js");
    assert!(enabled.enabled);
    assert_eq!(enabled.shim_paths.len(), 4);
    for (command, path) in MACOS_NODE_SHIM_NAMES.iter().zip(&enabled.shim_paths) {
      let contents = std::fs::read_to_string(path).expect("read Node.js shim");
      assert!(contents.contains("runtimes/node/current/bin"));
      assert!(contents.contains(command));
    }
    let profile = std::fs::read_to_string(&profile_path).expect("read enabled profile");
    assert_eq!(profile.matches(MACOS_SHELL_BLOCK_START).count(), 1);
    let rc = std::fs::read_to_string(&rc_path).expect("read enabled rc");
    assert_eq!(rc.matches(MACOS_SHELL_BLOCK_START).count(), 1);
    assert!(rc.ends_with(&format!(
      "{}\n",
      macos_shell_block(&enabled.bin_path).unwrap()
    )));

    enable_macos_terminal_node(&data_root, &profile_path, &rc_path)
      .expect("repair terminal Node.js");
    let repaired = std::fs::read_to_string(&profile_path).expect("read repaired profile");
    assert_eq!(repaired.matches(MACOS_SHELL_BLOCK_START).count(), 1);
    let repaired_rc = std::fs::read_to_string(&rc_path).expect("read repaired rc");
    assert_eq!(repaired_rc.matches(MACOS_SHELL_BLOCK_START).count(), 1);

    let disabled = disable_macos_terminal_node(&data_root, &profile_path, &rc_path)
      .expect("disable macOS terminal Node.js");
    assert!(!disabled.enabled);
    assert!(disabled.shim_paths.iter().all(|path| !path.exists()));
    assert_eq!(
      std::fs::read_to_string(&profile_path).expect("read disabled profile"),
      "export PATH='/usr/bin'\n"
    );
    assert_eq!(
      std::fs::read_to_string(&rc_path).expect("read disabled rc"),
      "export PATH='/opt/homebrew/bin':\"$PATH\"\n"
    );
    std::fs::remove_dir_all(root).expect("remove terminal Node.js fixture");
  }

  #[test]
  #[cfg(target_os = "macos")]
  fn rejects_an_incomplete_macos_terminal_node_block() {
    let error = remove_macos_shell_block(MACOS_SHELL_BLOCK_START)
      .expect_err("reject incomplete Node.js shell block");
    assert!(error
      .to_string()
      .contains("incomplete fabDev Node.js block"));
  }
}
