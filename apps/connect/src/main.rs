#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
  windows::run()
}

#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
  anyhow::bail!("fabDev Connect can only run on Windows")
}
