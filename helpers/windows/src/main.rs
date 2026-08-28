#[cfg(not(windows))]
use anyhow::bail;
use anyhow::Result;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() -> Result<()> {
  windows::run()
}

#[cfg(not(windows))]
fn main() -> Result<()> {
  bail!("fabDev Windows Helper can only run on Windows")
}
