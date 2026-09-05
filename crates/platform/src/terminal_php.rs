use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use directories::BaseDirs;

use crate::PlatformError;

#[cfg(any(target_os = "macos", test))]
const SHELL_BLOCK_START: &str = "# >>> fabDev terminal PHP >>>";
#[cfg(any(target_os = "macos", test))]
const SHELL_BLOCK_END: &str = "# <<< fabDev terminal PHP <<<";
#[cfg(any(target_os = "macos", test))]
const HERD_BLOCK_START: &str = "# >>> fabDev disabled Herd PHP PATH >>>";
#[cfg(any(target_os = "macos", test))]
const HERD_BLOCK_END: &str = "# <<< fabDev disabled Herd PHP PATH <<<";
#[cfg(any(windows, test))]
const WINDOWS_PHP_SHIM: &str = r#"@echo off
setlocal EnableExtensions EnableDelayedExpansion
set "FABDEV_DATA_ROOT=%~dp0.."
set "FABDEV_VERSION_FILE=%FABDEV_DATA_ROOT%\runtimes\php\current.version"
if not exist "%FABDEV_VERSION_FILE%" (
  echo fabDev global PHP is not installed or selected. 1>&2
  exit /b 127
)
set /p FABDEV_PHP_VERSION=<"%FABDEV_VERSION_FILE%"
echo(!FABDEV_PHP_VERSION!| %SystemRoot%\System32\findstr.exe /r /x "[0-9][0-9.]*" >nul
if errorlevel 1 (
  echo fabDev global PHP version state is invalid. 1>&2
  exit /b 127
)
set "FABDEV_PHP=%FABDEV_DATA_ROOT%\runtimes\php\!FABDEV_PHP_VERSION!\php.exe"
if not exist "%FABDEV_PHP%" (
  echo fabDev global PHP is not installed or selected. 1>&2
  exit /b 127
)
"%FABDEV_PHP%" %*
exit /b %ERRORLEVEL%
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalPhpIntegrationState {
  pub enabled: bool,
  pub bin_path: PathBuf,
  pub shim_path: PathBuf,
}

pub fn terminal_php_state(
  data_root: impl AsRef<Path>,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  platform_terminal_php_state(data_root.as_ref())
}

pub fn enable_terminal_php(
  data_root: impl AsRef<Path>,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  platform_enable_terminal_php(data_root.as_ref())
}

pub fn disable_terminal_php(
  data_root: impl AsRef<Path>,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  platform_disable_terminal_php(data_root.as_ref())
}

pub(crate) fn terminal_bin_path(data_root: &Path) -> PathBuf {
  data_root.join("bin")
}

#[cfg(target_os = "macos")]
fn platform_terminal_php_state(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  macos_terminal_php_state(data_root, &profile_path, &rc_path)
}

#[cfg(target_os = "macos")]
fn platform_enable_terminal_php(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  enable_macos_terminal_php(data_root, &profile_path, &rc_path)
}

#[cfg(target_os = "macos")]
fn platform_disable_terminal_php(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let (profile_path, rc_path) = macos_shell_paths()?;
  disable_macos_terminal_php(data_root, &profile_path, &rc_path)
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
fn macos_terminal_php_state(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  let shim_path = bin_path.join("php");
  let contents = match std::fs::read_to_string(profile_path) {
    Ok(contents) => contents,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
    Err(error) => return Err(error.into()),
  };
  let rc_contents = read_optional_text(rc_path)?;
  let expected_block = macos_shell_block(&bin_path)?;
  Ok(TerminalPhpIntegrationState {
    enabled: shim_path.is_file()
      && contents.contains(&expected_block)
      && !has_active_herd_php_path(&contents)
      && !has_active_herd_php_path(&rc_contents),
    bin_path,
    shim_path,
  })
}

#[cfg(target_os = "macos")]
fn enable_macos_terminal_php(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  let existing_profile = read_optional_text(profile_path)?;
  let existing_rc = read_optional_text(rc_path)?;
  let mut updated_profile = mark_herd_php_paths(&remove_shell_block(&existing_profile)?)?;
  let updated_rc = mark_herd_php_paths(&existing_rc)?;
  if !updated_profile.is_empty() && !updated_profile.ends_with('\n') {
    updated_profile.push('\n');
  }
  if !updated_profile.is_empty() && !updated_profile.ends_with("\n\n") {
    updated_profile.push('\n');
  }
  updated_profile.push_str(&macos_shell_block(&bin_path)?);
  updated_profile.push('\n');
  std::fs::create_dir_all(&bin_path)?;
  write_macos_php_shim(&bin_path.join("php"))?;
  if updated_rc != existing_rc {
    atomic_write(rc_path, updated_rc.as_bytes())?;
  }
  if let Err(error) = atomic_write(profile_path, updated_profile.as_bytes()) {
    if updated_rc != existing_rc {
      let _ = atomic_write(rc_path, existing_rc.as_bytes());
    }
    return Err(error);
  }
  macos_terminal_php_state(data_root, profile_path, rc_path)
}

#[cfg(target_os = "macos")]
fn disable_macos_terminal_php(
  data_root: &Path,
  profile_path: &Path,
  rc_path: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let existing_profile = read_optional_text(profile_path)?;
  let existing_rc = read_optional_text(rc_path)?;
  let updated_profile = restore_herd_php_paths(&remove_shell_block(&existing_profile)?)?;
  let updated_rc = restore_herd_php_paths(&existing_rc)?;
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
  let shim_path = terminal_bin_path(data_root).join("php");
  match std::fs::remove_file(&shim_path) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  macos_terminal_php_state(data_root, profile_path, rc_path)
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
    "{SHELL_BLOCK_START}\nexport PATH='{escaped}':\"$PATH\"\n{SHELL_BLOCK_END}"
  ))
}

#[cfg(target_os = "macos")]
fn write_macos_php_shim(shim_path: &Path) -> Result<(), PlatformError> {
  use std::os::unix::fs::PermissionsExt;

  const SHIM: &str = r#"#!/bin/sh
# Resolve the selected fabDev PHP Runtime on every invocation.
FABDEV_BIN_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
FABDEV_DATA_ROOT=$(dirname -- "$FABDEV_BIN_DIR")
FABDEV_PHP="$FABDEV_DATA_ROOT/runtimes/php/current/bin/php"
if [ ! -x "$FABDEV_PHP" ]; then
  printf '%s\n' 'fabDev global PHP is not installed or selected.' >&2
  exit 127
fi
exec "$FABDEV_PHP" "$@"
"#;

  atomic_write(shim_path, SHIM.as_bytes())?;
  let mut permissions = std::fs::metadata(shim_path)?.permissions();
  permissions.set_mode(0o755);
  std::fs::set_permissions(shim_path, permissions)?;
  Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn is_herd_php_path_line(line: &str) -> bool {
  let trimmed = line.trim();
  if trimmed.starts_with('#') {
    return false;
  }
  let normalized = trimmed.replace('\\', "/").to_lowercase();
  normalized.contains("path") && normalized.contains("herd/bin")
}

#[cfg(any(target_os = "macos", test))]
fn has_active_herd_php_path(contents: &str) -> bool {
  contents.lines().any(is_herd_php_path_line)
}

#[cfg(any(target_os = "macos", test))]
fn mark_herd_php_paths(contents: &str) -> Result<String, PlatformError> {
  let restored = restore_herd_php_paths(contents)?;
  let mut updated = String::with_capacity(restored.len());
  for segment in restored.split_inclusive('\n') {
    let (line, ending) = match segment.strip_suffix('\n') {
      Some(line) => (
        line.strip_suffix('\r').unwrap_or(line),
        &segment[line.len()..],
      ),
      None => (segment, ""),
    };
    if is_herd_php_path_line(line) {
      updated.push_str(HERD_BLOCK_START);
      updated.push_str(ending);
      updated.push_str("# ");
      updated.push_str(line);
      updated.push_str(ending);
      updated.push_str(HERD_BLOCK_END);
      updated.push_str(ending);
    } else {
      updated.push_str(segment);
    }
  }
  Ok(updated)
}

#[cfg(any(target_os = "macos", test))]
fn restore_herd_php_paths(contents: &str) -> Result<String, PlatformError> {
  let mut updated = contents.to_owned();
  loop {
    let Some(start) = updated.find(HERD_BLOCK_START) else {
      return Ok(updated);
    };
    let body_start = start + HERD_BLOCK_START.len();
    let Some(relative_end) = updated[body_start..].find(HERD_BLOCK_END) else {
      return Err(PlatformError::InvalidTerminalIntegration(
        "the shell profile contains an incomplete Herd PHP PATH block".to_owned(),
      ));
    };
    let marker_end = body_start + relative_end;
    let body = updated[body_start..marker_end].trim_matches(['\r', '\n']);
    let original = body.strip_prefix("# ").ok_or_else(|| {
      PlatformError::InvalidTerminalIntegration(
        "the disabled Herd PHP PATH block is not recoverable".to_owned(),
      )
    })?;
    if original.contains('\n') || original.contains('\r') {
      return Err(PlatformError::InvalidTerminalIntegration(
        "the disabled Herd PHP PATH block contains multiple lines".to_owned(),
      ));
    }
    let mut end = marker_end + HERD_BLOCK_END.len();
    let ending = if updated[end..].starts_with("\r\n") {
      end += 2;
      "\r\n"
    } else if updated[end..].starts_with('\n') {
      end += 1;
      "\n"
    } else {
      ""
    };
    updated.replace_range(start..end, &format!("{original}{ending}"));
  }
}

#[cfg(any(target_os = "macos", test))]
fn remove_shell_block(contents: &str) -> Result<String, PlatformError> {
  let mut updated = contents.to_owned();
  loop {
    let Some(start) = updated.find(SHELL_BLOCK_START) else {
      return Ok(updated);
    };
    let search_from = start + SHELL_BLOCK_START.len();
    let Some(relative_end) = updated[search_from..].find(SHELL_BLOCK_END) else {
      return Err(PlatformError::InvalidTerminalIntegration(
        "the shell profile contains an incomplete fabDev block".to_owned(),
      ));
    };
    let mut end = search_from + relative_end + SHELL_BLOCK_END.len();
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

#[cfg(windows)]
fn platform_terminal_php_state(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  let shim_path = bin_path.join("php.cmd");
  let user_path = windows_user_path()?;
  Ok(TerminalPhpIntegrationState {
    enabled: shim_path.is_file()
      && path_contains(&user_path, &bin_path)
      && !path_has_herd_php(&user_path),
    bin_path,
    shim_path,
  })
}

#[cfg(windows)]
fn platform_enable_terminal_php(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  std::fs::create_dir_all(&bin_path)?;
  write_windows_php_shim(&bin_path.join("php.cmd"))?;
  let user_path = windows_user_path()?;
  let (without_herd, removed_herd) = remove_herd_php_entries(&user_path);
  if !removed_herd.is_empty() {
    save_windows_herd_path_backup(data_root, &removed_herd)?;
  }
  let updated = if path_contains(&without_herd, &bin_path) {
    without_herd
  } else {
    prepend_path_entry(&without_herd, &bin_path)
  };
  if updated != user_path {
    set_windows_user_path(&updated)?;
    broadcast_environment_change();
  }
  platform_terminal_php_state(data_root)
}

#[cfg(windows)]
fn platform_disable_terminal_php(
  data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  let bin_path = terminal_bin_path(data_root);
  let shim_path = bin_path.join("php.cmd");
  match std::fs::remove_file(&shim_path) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  let user_path = windows_user_path()?;
  let without_fabdev = if windows_node_shims_exist(&bin_path) {
    user_path.clone()
  } else {
    remove_path_entry(&user_path, &bin_path)
  };
  let restored_herd = load_windows_herd_path_backup(data_root)?;
  let updated = restore_path_entries(&without_fabdev, &restored_herd);
  if updated != user_path {
    set_windows_user_path(&updated)?;
    broadcast_environment_change();
  }
  remove_windows_herd_path_backup(data_root)?;
  platform_terminal_php_state(data_root)
}

#[cfg(windows)]
fn write_windows_php_shim(shim_path: &Path) -> Result<(), PlatformError> {
  std::fs::write(shim_path, WINDOWS_PHP_SHIM)?;
  Ok(())
}

#[cfg(windows)]
pub(crate) fn windows_user_path() -> Result<String, PlatformError> {
  use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
  use winreg::RegKey;

  let current_user = RegKey::predef(HKEY_CURRENT_USER);
  let environment = current_user.open_subkey_with_flags("Environment", KEY_READ)?;
  match environment.get_value("Path") {
    Ok(value) => Ok(value),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(windows)]
pub(crate) fn set_windows_user_path(value: &str) -> Result<(), PlatformError> {
  use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ};
  use winreg::{RegKey, RegValue};

  let current_user = RegKey::predef(HKEY_CURRENT_USER);
  let environment = current_user.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;
  let value_type = environment
    .get_raw_value("Path")
    .map(|value| value.vtype)
    .unwrap_or(REG_EXPAND_SZ);
  let value_type = if value_type == REG_SZ || value_type == REG_EXPAND_SZ {
    value_type
  } else {
    REG_EXPAND_SZ
  };
  let mut words = value.encode_utf16().collect::<Vec<_>>();
  words.push(0);
  let bytes = words
    .iter()
    .flat_map(|word| word.to_le_bytes())
    .collect::<Vec<_>>();
  environment.set_raw_value(
    "Path",
    &RegValue {
      bytes,
      vtype: value_type,
    },
  )?;
  Ok(())
}

#[cfg(windows)]
pub(crate) fn broadcast_environment_change() {
  use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
  };

  let environment = "Environment\0".encode_utf16().collect::<Vec<_>>();
  unsafe {
    SendMessageTimeoutW(
      HWND_BROADCAST,
      WM_SETTINGCHANGE,
      0,
      environment.as_ptr() as isize,
      SMTO_ABORTIFHUNG,
      5_000,
      std::ptr::null_mut(),
    );
  }
}

#[cfg(windows)]
fn windows_herd_path_backup(data_root: &Path) -> PathBuf {
  data_root.join("state/terminal-php-herd-path.txt")
}

#[cfg(windows)]
fn save_windows_herd_path_backup(
  data_root: &Path,
  entries: &[(usize, String)],
) -> Result<(), PlatformError> {
  let path = windows_herd_path_backup(data_root);
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  if entries
    .iter()
    .any(|(_, entry)| entry.contains('\r') || entry.contains('\n') || entry.contains('\t'))
  {
    return Err(PlatformError::InvalidTerminalIntegration(
      "a Herd PHP PATH entry contains a line break".to_owned(),
    ));
  }
  let contents = entries
    .iter()
    .map(|(index, entry)| format!("{index}\t{entry}"))
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(path, contents)?;
  Ok(())
}

#[cfg(windows)]
fn load_windows_herd_path_backup(data_root: &Path) -> Result<Vec<(usize, String)>, PlatformError> {
  match std::fs::read_to_string(windows_herd_path_backup(data_root)) {
    Ok(contents) => contents
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(|line| {
        let (index, entry) = line.split_once('\t').ok_or_else(|| {
          PlatformError::InvalidTerminalIntegration(
            "the Windows Herd PHP PATH backup is invalid".to_owned(),
          )
        })?;
        let index = index.parse::<usize>().map_err(|_| {
          PlatformError::InvalidTerminalIntegration(
            "the Windows Herd PHP PATH backup index is invalid".to_owned(),
          )
        })?;
        Ok((index, entry.to_owned()))
      })
      .collect(),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(windows)]
fn remove_windows_herd_path_backup(data_root: &Path) -> Result<(), PlatformError> {
  match std::fs::remove_file(windows_herd_path_backup(data_root)) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(any(windows, test))]
fn normalize_path_entry(value: &str) -> String {
  value
    .trim()
    .trim_matches('"')
    .trim_end_matches(['\\', '/'])
    .replace('/', "\\")
    .to_lowercase()
}

#[cfg(any(windows, test))]
pub(crate) fn path_contains(value: &str, path: &Path) -> bool {
  let expected = normalize_path_entry(&path.to_string_lossy());
  value
    .split(';')
    .any(|entry| normalize_path_entry(entry) == expected)
}

#[cfg(any(windows, test))]
fn is_herd_php_path_entry(entry: &str) -> bool {
  normalize_path_entry(entry).contains("\\herd\\bin")
}

#[cfg(any(windows, test))]
fn path_has_herd_php(value: &str) -> bool {
  value.split(';').any(is_herd_php_path_entry)
}

#[cfg(any(windows, test))]
fn remove_herd_php_entries(value: &str) -> (String, Vec<(usize, String)>) {
  let mut kept = Vec::new();
  let mut removed = Vec::new();
  for (index, entry) in value
    .split(';')
    .filter(|entry| !entry.trim().is_empty())
    .enumerate()
  {
    if is_herd_php_path_entry(entry) {
      removed.push((index, entry.to_owned()));
    } else {
      kept.push(entry);
    }
  }
  (kept.join(";"), removed)
}

#[cfg(any(windows, test))]
fn restore_path_entries(value: &str, entries: &[(usize, String)]) -> String {
  let mut restored = value
    .split(';')
    .filter(|entry| !entry.trim().is_empty())
    .map(str::to_owned)
    .collect::<Vec<_>>();
  for (index, entry) in entries {
    if !restored
      .iter()
      .any(|current| normalize_path_entry(current) == normalize_path_entry(entry))
    {
      restored.insert((*index).min(restored.len()), entry.clone());
    }
  }
  restored.join(";")
}

#[cfg(windows)]
pub(crate) fn prepend_path_entry(value: &str, path: &Path) -> String {
  let path = path.to_string_lossy();
  if value.trim().is_empty() {
    path.into_owned()
  } else {
    format!("{path};{value}")
  }
}

#[cfg(any(windows, test))]
pub(crate) fn remove_path_entry(value: &str, path: &Path) -> String {
  let expected = normalize_path_entry(&path.to_string_lossy());
  value
    .split(';')
    .filter(|entry| !entry.trim().is_empty())
    .filter(|entry| normalize_path_entry(entry) != expected)
    .collect::<Vec<_>>()
    .join(";")
}

#[cfg(windows)]
fn windows_node_shims_exist(bin_path: &Path) -> bool {
  ["node.cmd", "npm.cmd", "npx.cmd", "corepack.cmd"]
    .iter()
    .any(|name| bin_path.join(name).is_file())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_terminal_php_state(
  _data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  Err(PlatformError::Unsupported)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_enable_terminal_php(
  _data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  Err(PlatformError::Unsupported)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_disable_terminal_php(
  _data_root: &Path,
) -> Result<TerminalPhpIntegrationState, PlatformError> {
  Err(PlatformError::Unsupported)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[cfg(target_os = "macos")]
  fn rejects_invalid_php_profiles_before_writing_shims() {
    for (case, (profile, rc)) in [
      (SHELL_BLOCK_START, "export EDITOR=vi\n"),
      ("export EDITOR=vi\n", HERD_BLOCK_START),
    ]
    .into_iter()
    .enumerate()
    {
      for existing in [false, true] {
        let root = std::env::temp_dir().join(format!(
          "fabdev-php-preflight-{}-{case}-{existing}",
          std::process::id()
        ));
        let data_root = root.join("data");
        let profile_path = root.join(".zprofile");
        let rc_path = root.join(".zshrc");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&profile_path, profile).unwrap();
        std::fs::write(&rc_path, rc).unwrap();
        let commands: &[&str] = &["php"];
        if existing {
          std::fs::create_dir_all(data_root.join("bin")).unwrap();
          for command in commands {
            std::fs::write(data_root.join("bin").join(command), "original shim\n").unwrap();
          }
        }
        let result = enable_macos_terminal_php(&data_root, &profile_path, &rc_path);
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
          "invalid profile must not create or overwrite php shims"
        );
      }
    }
  }

  #[test]
  fn path_entry_matching_is_case_insensitive_and_separator_independent() {
    let path = Path::new(r"C:\Users\Jimmy\AppData\Local\FabDev\bin");
    assert!(path_contains(
      r"C:\Windows;C:/USERS/JIMMY/AppData/Local/FabDev/bin/",
      path
    ));
    assert!(!path_contains(r"C:\Windows;C:\FabDev\other", path));
  }

  #[test]
  fn removing_path_entry_preserves_unrelated_entries() {
    let path = Path::new(r"C:\FabDev\bin");
    assert_eq!(
      remove_path_entry(r"C:\Herd\bin;C:\FabDev\bin;C:\Windows", path),
      r"C:\Herd\bin;C:\Windows"
    );
  }

  #[test]
  fn removes_and_restores_windows_herd_php_path_entries() {
    let original = r"C:\Windows;C:\Users\Jimmy\AppData\Local\Herd\bin;C:\Tools";
    let (without_herd, removed) = remove_herd_php_entries(original);
    assert_eq!(without_herd, r"C:\Windows;C:\Tools");
    assert_eq!(
      removed,
      vec![(1, r"C:\Users\Jimmy\AppData\Local\Herd\bin".to_owned())]
    );
    assert!(!path_has_herd_php(&without_herd));
    assert_eq!(restore_path_entries(&without_herd, &removed), original);
  }

  #[test]
  fn windows_shim_resolves_the_current_version_on_each_run() {
    assert!(WINDOWS_PHP_SHIM.contains(r"runtimes\php\current.version"));
    assert!(WINDOWS_PHP_SHIM.contains("!FABDEV_PHP_VERSION!\\php.exe"));
    assert!(!WINDOWS_PHP_SHIM.contains(r"runtimes\php\8."));
  }

  #[test]
  fn removes_only_complete_managed_shell_blocks() {
    let profile = format!(
      "export PATH=/opt/homebrew/bin:$PATH\n{SHELL_BLOCK_START}\nexport PATH='/tmp/fabDev/bin':\"$PATH\"\n{SHELL_BLOCK_END}\nexport HERD=1\n"
    );
    assert_eq!(
      remove_shell_block(&profile).expect("remove managed block"),
      "export PATH=/opt/homebrew/bin:$PATH\nexport HERD=1\n"
    );
  }

  #[test]
  fn rejects_incomplete_managed_shell_block() {
    let profile = format!("export PATH=/usr/bin\n{SHELL_BLOCK_START}\n");
    assert!(matches!(
      remove_shell_block(&profile),
      Err(PlatformError::InvalidTerminalIntegration(_))
    ));
  }

  #[test]
  fn marks_and_restores_only_herd_php_path_lines() {
    let shell = "export NVM_DIR=/Herd/config/nvm\nexport PATH=\"/Users/Jimmy/Library/Application Support/Herd/bin/\":$PATH\nexport PATH=/opt/homebrew/bin:$PATH\n";
    let marked = mark_herd_php_paths(shell).expect("mark Herd PHP PATH");
    assert!(!has_active_herd_php_path(&marked));
    assert!(marked.contains(HERD_BLOCK_START));
    assert!(marked.contains("export NVM_DIR=/Herd/config/nvm"));
    assert!(marked.contains("export PATH=/opt/homebrew/bin:$PATH"));
    assert_eq!(
      mark_herd_php_paths(&marked).expect("repair Herd PHP PATH marker"),
      marked
    );
    assert_eq!(
      restore_herd_php_paths(&marked).expect("restore Herd PHP PATH"),
      shell
    );
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn enables_repairs_and_disables_macos_terminal_php() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = std::env::temp_dir().join(format!(
      "fabdev-terminal-php-{}-{}",
      std::process::id(),
      std::thread::current().name().unwrap_or("test")
    ));
    let data_root = root.join("FabDev Application Support");
    let profile = root.join(".zprofile");
    let rc = root.join(".zshrc");
    std::fs::create_dir_all(&root).expect("create fixture root");
    std::fs::write(&profile, "export PATH=/opt/homebrew/bin:$PATH\n")
      .expect("write fixture profile");
    std::fs::write(
      &rc,
      "export NVM_DIR=/Herd/config/nvm\nexport PATH=\"/Users/Jimmy/Library/Application Support/Herd/bin/\":$PATH\n",
    )
    .expect("write fixture rc");

    let enabled =
      enable_macos_terminal_php(&data_root, &profile, &rc).expect("enable terminal PHP");
    assert!(enabled.enabled);
    assert!(enabled.shim_path.is_file());
    let first_profile = std::fs::read_to_string(&profile).expect("read enabled profile");
    assert!(first_profile.contains("export PATH=/opt/homebrew/bin:$PATH"));
    assert_eq!(first_profile.matches(SHELL_BLOCK_START).count(), 1);
    let first_rc = std::fs::read_to_string(&rc).expect("read enabled rc");
    assert!(first_rc.contains(HERD_BLOCK_START));
    assert!(!has_active_herd_php_path(&first_rc));
    assert!(first_rc.contains("export NVM_DIR=/Herd/config/nvm"));

    let php_root = data_root.join("runtimes/php");
    for version in ["8.2.33", "8.4.24"] {
      let binary = php_root.join(version).join("bin/php");
      std::fs::create_dir_all(binary.parent().expect("PHP binary parent"))
        .expect("create PHP Runtime fixture");
      std::fs::write(
        &binary,
        format!("#!/bin/sh\nprintf '{version}:%s\\n' \"$1\"\n"),
      )
      .expect("write PHP fixture");
      let mut permissions = std::fs::metadata(&binary)
        .expect("read PHP fixture metadata")
        .permissions();
      permissions.set_mode(0o755);
      std::fs::set_permissions(&binary, permissions).expect("make PHP fixture executable");
    }
    let current = php_root.join("current");
    symlink("8.2.33", &current).expect("select PHP 8.2 fixture");
    let php82 = std::process::Command::new(&enabled.shim_path)
      .arg("fixture.php")
      .output()
      .expect("run PHP 8.2 through shim");
    assert_eq!(
      String::from_utf8_lossy(&php82.stdout),
      "8.2.33:fixture.php\n"
    );
    std::fs::remove_file(&current).expect("remove PHP 8.2 selection");
    symlink("8.4.24", &current).expect("select PHP 8.4 fixture");
    let php84 = std::process::Command::new(&enabled.shim_path)
      .arg("fixture.php")
      .output()
      .expect("run PHP 8.4 through shim");
    assert_eq!(
      String::from_utf8_lossy(&php84.stdout),
      "8.4.24:fixture.php\n"
    );

    enable_macos_terminal_php(&data_root, &profile, &rc).expect("repair terminal PHP");
    let repaired_profile = std::fs::read_to_string(&profile).expect("read repaired profile");
    assert_eq!(repaired_profile.matches(SHELL_BLOCK_START).count(), 1);

    let disabled =
      disable_macos_terminal_php(&data_root, &profile, &rc).expect("disable terminal PHP");
    assert!(!disabled.enabled);
    assert!(!disabled.shim_path.exists());
    assert_eq!(
      std::fs::read_to_string(&profile).expect("read disabled profile"),
      "export PATH=/opt/homebrew/bin:$PATH\n"
    );
    assert_eq!(
      std::fs::read_to_string(&rc).expect("read disabled rc"),
      "export NVM_DIR=/Herd/config/nvm\nexport PATH=\"/Users/Jimmy/Library/Application Support/Herd/bin/\":$PATH\n"
    );
    std::fs::remove_dir_all(root).expect("remove fixture root");
  }
}
