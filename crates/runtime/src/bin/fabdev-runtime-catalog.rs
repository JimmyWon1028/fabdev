use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use fabdev_runtime::{
  generate_community_php_catalog, parse_and_validate_runtime_catalog, CommunityPhpCatalogInput,
  RuntimeCatalogValidation, RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
};

fn usage() -> &'static str {
  "Usage:\n  fabdev-runtime-catalog generate <release-version> <catalog-sequence> <generated-at> <expires-at> <minimum-app-version> <macos-package> <windows-package> <output>\n  fabdev-runtime-catalog validate <catalog> <current-app-version>"
}

fn now_unix_seconds() -> Result<i64, Box<dyn Error>> {
  Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
}

fn generate(args: &[String]) -> Result<(), Box<dyn Error>> {
  if args.len() != 8 {
    return Err(usage().into());
  }
  let sequence = args[1].parse::<u64>()?;
  let contents = generate_community_php_catalog(&CommunityPhpCatalogInput {
    release_version: &args[0],
    catalog_sequence: sequence,
    generated_at: &args[2],
    expires_at: &args[3],
    minimum_app_version: &args[4],
    macos_arm64_package: Path::new(&args[5]),
    windows_x64_package: Path::new(&args[6]),
    now_unix_seconds: now_unix_seconds()?,
  })?;
  let output = Path::new(&args[7]);
  if let Some(parent) = output.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let mut file = OpenOptions::new().write(true).create_new(true).open(output)?;
  file.write_all(&contents)?;
  println!("Generated {}", output.display());
  Ok(())
}

fn validate(args: &[String]) -> Result<(), Box<dyn Error>> {
  if args.len() != 2 {
    return Err(usage().into());
  }
  let contents = std::fs::read(&args[0])?;
  let validated = parse_and_validate_runtime_catalog(
    &contents,
    &RuntimeCatalogValidation {
      current_app_version: &args[1],
      current_agent_protocol_version: RUNTIME_CATALOG_MINIMUM_PROTOCOL_VERSION,
      now_unix_seconds: now_unix_seconds()?,
      accepted_catalog: None,
    },
  )?;
  println!(
    "Validated Catalog sequence {} with SHA-256 {}",
    validated.catalog.catalog_sequence, validated.sha256
  );
  Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
  let mut args = std::env::args().skip(1).collect::<Vec<_>>();
  if args.is_empty() {
    return Err(usage().into());
  }
  let command = args.remove(0);
  match command.as_str() {
    "generate" => generate(&args),
    "validate" => validate(&args),
    "--help" | "-h" => {
      println!("{}", usage());
      Ok(())
    }
    _ => Err(format!("Unknown command: {command}\n{}", usage()).into()),
  }
}
