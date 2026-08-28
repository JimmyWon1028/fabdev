use fabdev_runtime::{RuntimeCatalog, RuntimeRelease};

pub fn find_release<'a>(
  catalog: &'a RuntimeCatalog,
  name: &str,
  version: &str,
  platform: &str,
  architecture: &str,
) -> Option<&'a RuntimeRelease> {
  catalog.runtimes.iter().find(|release| {
    release.name == name
      && release.version == version
      && release.platform == platform
      && release.architecture == architecture
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn filters_release_by_platform_and_architecture() {
    let catalog = RuntimeCatalog {
      schema_version: 1,
      generated_at: "2026-08-22T00:00:00Z".to_owned(),
      runtimes: vec![RuntimeRelease {
        name: "php".to_owned(),
        version: "8.2.33".to_owned(),
        platform: "macos".to_owned(),
        architecture: "arm64".to_owned(),
        url: "https://example.invalid/php.tar.zst".to_owned(),
        size: 1,
        sha256: "00".repeat(32),
        signature: "development".to_owned(),
      }],
    };
    assert!(find_release(&catalog, "php", "8.2.33", "macos", "arm64").is_some());
    assert!(find_release(&catalog, "php", "8.2.33", "windows", "x64").is_none());
  }
}
