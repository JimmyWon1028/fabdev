use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::Write;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
  MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

const MANAGED_START: &str = "# BEGIN FABDEV MANAGED";
const MANAGED_END: &str = "# END FABDEV MANAGED";

#[derive(Debug, Parser)]
#[command(name = "fabdev-windows-helper", version)]
struct Arguments {
  #[arg(long, hide = true)]
  elevated: bool,
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  SyncHosts {
    domains: Vec<String>,
  },
  TrustCa {
    #[arg(long)]
    certificate: PathBuf,
  },
  UntrustCa {
    #[arg(long)]
    certificate: PathBuf,
  },
}

pub fn run() -> Result<()> {
  let arguments = Arguments::parse();
  match &arguments.command {
    Command::SyncHosts { domains } => {
      let domains = normalize_domains(domains)?;
      if !arguments.elevated {
        return elevate(
          std::iter::once(OsString::from("sync-hosts"))
            .chain(domains.iter().map(OsString::from))
            .collect(),
        );
      }
      sync_hosts(&hosts_path()?, &domains)
    }
    Command::TrustCa { certificate } => {
      let certificate = validate_certificate_path(certificate)?;
      if !arguments.elevated {
        if ca_is_trusted(&certificate)? {
          return Ok(());
        }
        return elevate(vec![
          OsString::from("trust-ca"),
          OsString::from("--certificate"),
          certificate.into_os_string(),
        ]);
      }
      trust_ca(&certificate)
    }
    Command::UntrustCa { certificate } => {
      let certificate = validate_certificate_path(certificate)?;
      if !arguments.elevated {
        return elevate(vec![
          OsString::from("untrust-ca"),
          OsString::from("--certificate"),
          certificate.into_os_string(),
        ]);
      }
      untrust_ca(&certificate)
    }
  }
}

fn normalize_domains(domains: &[String]) -> Result<Vec<String>> {
  if domains.len() > 256 {
    bail!("fabDev manages at most 256 .test domains");
  }
  let mut normalized = domains.to_vec();
  normalized.sort();
  normalized.dedup();
  for domain in &normalized {
    let valid = domain.ends_with(".test")
      && domain.len() <= 253
      && domain.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
      })
      && domain
        .split('.')
        .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'));
    if !valid {
      bail!("invalid managed domain: {domain}");
    }
  }
  Ok(normalized)
}

fn hosts_path() -> Result<PathBuf> {
  let system_root = std::env::var_os("SystemRoot").context("SystemRoot is not defined")?;
  Ok(PathBuf::from(system_root).join("System32/drivers/etc/hosts"))
}

fn elevate(arguments: Vec<OsString>) -> Result<()> {
  let executable = std::env::current_exe().context("unable to locate Windows Helper")?;
  let mut parameters = Vec::new();
  for (index, argument) in std::iter::once(OsString::from("--elevated"))
    .chain(arguments)
    .enumerate()
  {
    if index > 0 {
      parameters.push(b' ' as u16);
    }
    parameters.extend(quote_windows_argument(&argument));
  }
  parameters.push(0);
  let verb = wide("runas");
  let executable = wide(executable.as_os_str());
  let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
  info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
  info.fMask = SEE_MASK_NOCLOSEPROCESS;
  info.lpVerb = verb.as_ptr();
  info.lpFile = executable.as_ptr();
  info.lpParameters = parameters.as_ptr();
  info.nShow = SW_HIDE;
  let launched = unsafe { ShellExecuteExW(&mut info) };
  if launched == 0 || info.hProcess.is_null() {
    bail!("UAC elevation was rejected or failed: {}", unsafe {
      GetLastError()
    });
  }
  let process = ProcessHandle(info.hProcess);
  unsafe { WaitForSingleObject(process.0, INFINITE) };
  let mut exit_code = 1_u32;
  if unsafe { GetExitCodeProcess(process.0, &mut exit_code) } == 0 {
    bail!("unable to read elevated Helper exit code: {}", unsafe {
      GetLastError()
    });
  }
  if exit_code != 0 {
    bail!("elevated Windows Helper failed with exit code {exit_code}");
  }
  Ok(())
}

fn validate_certificate_path(path: &Path) -> Result<PathBuf> {
  let local_app_data = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not defined")?;
  let expected = PathBuf::from(local_app_data).join("FabDev/config/tls/ca.crt");
  let requested = path
    .canonicalize()
    .with_context(|| format!("unable to resolve local CA certificate: {}", path.display()))?;
  let expected = expected.canonicalize().with_context(|| {
    format!(
      "fabDev managed local CA certificate is missing: {}",
      expected.display()
    )
  })?;
  if requested != expected {
    bail!("refusing to manage a certificate outside fabDev managed storage");
  }
  let metadata = std::fs::metadata(&requested)?;
  if !metadata.is_file() || metadata.len() > 64 * 1_024 {
    bail!("fabDev local CA certificate is not a valid regular file");
  }
  let pem = std::fs::read_to_string(&requested)?;
  if !pem.contains("-----BEGIN CERTIFICATE-----") || !pem.contains("-----END CERTIFICATE-----") {
    bail!("fabDev local CA certificate is not PEM encoded");
  }
  let dump = run_certutil([OsStr::new("-dump"), requested.as_os_str()])?;
  if !dump.contains("fabDev Local Development CA") {
    bail!("certificate subject is not owned by fabDev");
  }
  Ok(requested)
}

fn trust_ca(certificate: &Path) -> Result<()> {
  run_certutil([
    OsStr::new("-user"),
    OsStr::new("-addstore"),
    OsStr::new("-f"),
    OsStr::new("Root"),
    certificate.as_os_str(),
  ])?;
  Ok(())
}

fn ca_is_trusted(certificate: &Path) -> Result<bool> {
  let fingerprint = certificate_sha1(certificate)?;
  let status = std::process::Command::new("certutil.exe")
    .args(["-user", "-store", "Root", &fingerprint])
    .status()
    .context("unable to inspect the Windows Current User Root store")?;
  Ok(status.success())
}

fn untrust_ca(certificate: &Path) -> Result<()> {
  let fingerprint = certificate_sha1(certificate)?;
  run_certutil([
    OsStr::new("-user"),
    OsStr::new("-delstore"),
    OsStr::new("Root"),
    OsStr::new(&fingerprint),
  ])?;
  Ok(())
}

fn certificate_sha1(certificate: &Path) -> Result<String> {
  let output = run_certutil([
    OsStr::new("-hashfile"),
    certificate.as_os_str(),
    OsStr::new("SHA1"),
  ])?;
  output
    .lines()
    .map(|line| {
      line
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect()
    })
    .find(|line: &String| line.len() == 40)
    .context("certutil did not return the local CA SHA-1 fingerprint")
}

fn run_certutil<I, S>(arguments: I) -> Result<String>
where
  I: IntoIterator<Item = S>,
  S: AsRef<OsStr>,
{
  let output = std::process::Command::new("certutil.exe")
    .args(arguments)
    .output()
    .context("unable to start certutil.exe")?;
  if !output.status.success() {
    bail!(
      "certutil.exe failed: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn quote_windows_argument(argument: &OsStr) -> Vec<u16> {
  let argument = argument.encode_wide().collect::<Vec<_>>();
  if argument.is_empty() {
    return vec![b'"' as u16, b'"' as u16];
  }
  if !argument
    .iter()
    .any(|character| *character <= b' ' as u16 || *character == b'"' as u16)
  {
    return argument;
  }
  let mut quoted = vec![b'"' as u16];
  let mut backslashes = 0;
  for character in argument {
    if character == b'\\' as u16 {
      backslashes += 1;
      continue;
    }
    if character == b'"' as u16 {
      quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
      quoted.push(b'"' as u16);
    } else {
      quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
      quoted.push(character);
    }
    backslashes = 0;
  }
  quoted.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
  quoted.push(b'"' as u16);
  quoted
}

fn sync_hosts(path: &Path, domains: &[String]) -> Result<()> {
  let existing = std::fs::read_to_string(path)
    .with_context(|| format!("unable to read Windows hosts file: {}", path.display()))?;
  let without_managed = remove_managed_block(&existing)?;
  let mut contents = without_managed.trim_end_matches(['\r', '\n']).to_owned();
  if !domains.is_empty() {
    contents.push_str("\r\n\r\n");
    contents.push_str(MANAGED_START);
    contents.push_str("\r\n");
    for domain in domains {
      contents.push_str("127.0.0.1 ");
      contents.push_str(domain);
      contents.push_str("\r\n");
    }
    contents.push_str(MANAGED_END);
  }
  contents.push_str("\r\n");

  let backup = path.with_file_name("hosts.fabdev.backup");
  std::fs::copy(path, &backup)
    .with_context(|| format!("unable to back up Windows hosts file: {}", backup.display()))?;
  let pending = path.with_file_name("hosts.fabdev.pending");
  let mut file = OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(&pending)?;
  file.write_all(contents.as_bytes())?;
  file.sync_all()?;
  drop(file);

  let pending_wide = wide(pending.as_os_str());
  let path_wide = wide(path.as_os_str());
  let moved = unsafe {
    MoveFileExW(
      pending_wide.as_ptr(),
      path_wide.as_ptr(),
      MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
    )
  };
  if moved == 0 {
    bail!("unable to replace Windows hosts file: {}", unsafe {
      GetLastError()
    });
  }
  Ok(())
}

fn remove_managed_block(contents: &str) -> Result<String> {
  let Some(start) = contents.find(MANAGED_START) else {
    if contents.contains(MANAGED_END) {
      bail!("Windows hosts file contains an incomplete fabDev managed block");
    }
    return Ok(contents.to_owned());
  };
  let remainder = &contents[start + MANAGED_START.len()..];
  let end_offset = remainder
    .find(MANAGED_END)
    .context("Windows hosts file contains an incomplete fabDev managed block")?;
  let end = start + MANAGED_START.len() + end_offset + MANAGED_END.len();
  if contents[end..].contains(MANAGED_START) || contents[end..].contains(MANAGED_END) {
    bail!("Windows hosts file contains multiple fabDev managed blocks");
  }
  let mut output = contents[..start].trim_end_matches(['\r', '\n']).to_owned();
  output.push_str(&contents[end..]);
  Ok(output)
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
  value
    .as_ref()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect()
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
  fn drop(&mut self) {
    unsafe {
      CloseHandle(self.0);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn replaces_only_the_managed_hosts_block() {
    let existing = "127.0.0.1 localhost\r\n# BEGIN FABDEV MANAGED\r\n127.0.0.1 old.test\r\n# END FABDEV MANAGED\r\n10.0.0.2 intranet\r\n";
    let result = remove_managed_block(existing).expect("remove managed block");

    assert!(result.contains("127.0.0.1 localhost"));
    assert!(result.contains("10.0.0.2 intranet"));
    assert!(!result.contains("old.test"));
  }

  #[test]
  fn accepts_only_normalized_test_domains() {
    assert!(normalize_domains(&["erp.test".to_owned(), "crm-2.test".to_owned()]).is_ok());
    assert!(normalize_domains(&["Example.test".to_owned()]).is_err());
    assert!(normalize_domains(&["example.com".to_owned()]).is_err());
  }

  #[test]
  fn quotes_windows_arguments_without_losing_backslashes() {
    let quoted = |value: &str| {
      String::from_utf16(&quote_windows_argument(OsStr::new(value))).expect("decode fixture")
    };
    assert_eq!(quoted("sync-hosts"), "sync-hosts");
    assert_eq!(
      quoted(r"C:\Users\Dev User\FabDev\ca.crt"),
      r#""C:\Users\Dev User\FabDev\ca.crt""#
    );
    assert_eq!(quoted(""), r#"""#);
  }
}
