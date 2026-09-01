use anyhow::{bail, Context, Result};

pub const SITE_MANAGED_START: &str = "# BEGIN FABDEV MANAGED";
pub const SITE_MANAGED_END: &str = "# END FABDEV MANAGED";
pub const PROXY_MANAGED_START: &str = "# BEGIN FABDEV PROXY MANAGED";
pub const PROXY_MANAGED_END: &str = "# END FABDEV PROXY MANAGED";

pub fn normalize_domains(domains: &[String]) -> Result<Vec<String>> {
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

pub fn update_managed_block(
  contents: &str,
  start_marker: &str,
  end_marker: &str,
  domains: &[String],
) -> Result<String> {
  let without_managed = remove_managed_block(contents, start_marker, end_marker)?;
  let mut output = without_managed.trim_end_matches(['\r', '\n']).to_owned();
  if !domains.is_empty() {
    output.push_str("\r\n\r\n");
    output.push_str(start_marker);
    output.push_str("\r\n");
    for domain in domains {
      output.push_str("127.0.0.1 ");
      output.push_str(domain);
      output.push_str("\r\n");
    }
    output.push_str(end_marker);
  }
  output.push_str("\r\n");
  Ok(output)
}

fn remove_managed_block(contents: &str, start_marker: &str, end_marker: &str) -> Result<String> {
  let Some(start) = contents.find(start_marker) else {
    if contents.contains(end_marker) {
      bail!("Windows hosts file contains an incomplete fabDev managed block");
    }
    return Ok(contents.to_owned());
  };
  let remainder = &contents[start + start_marker.len()..];
  let end_offset = remainder
    .find(end_marker)
    .context("Windows hosts file contains an incomplete fabDev managed block")?;
  let end = start + start_marker.len() + end_offset + end_marker.len();
  if contents[end..].contains(start_marker) || contents[end..].contains(end_marker) {
    bail!("Windows hosts file contains multiple fabDev managed blocks");
  }
  let mut output = contents[..start].trim_end_matches(['\r', '\n']).to_owned();
  output.push_str(&contents[end..]);
  Ok(output)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn proxy_update_preserves_the_site_managed_block() {
    let existing = "127.0.0.1 localhost\r\n# BEGIN FABDEV MANAGED\r\n127.0.0.1 demo.test\r\n# END FABDEV MANAGED\r\n";
    let result = update_managed_block(
      existing,
      PROXY_MANAGED_START,
      PROXY_MANAGED_END,
      &["lysm.test".to_owned()],
    )
    .expect("add Proxy managed block");

    assert!(result.contains("127.0.0.1 demo.test"));
    assert!(result.contains("127.0.0.1 lysm.test"));
    assert_eq!(result.matches(SITE_MANAGED_START).count(), 1);
    assert_eq!(result.matches(PROXY_MANAGED_START).count(), 1);
  }

  #[test]
  fn site_update_preserves_the_proxy_managed_block() {
    let existing =
      "# BEGIN FABDEV PROXY MANAGED\r\n127.0.0.1 lysm.test\r\n# END FABDEV PROXY MANAGED\r\n";
    let result = update_managed_block(
      existing,
      SITE_MANAGED_START,
      SITE_MANAGED_END,
      &["demo.test".to_owned()],
    )
    .expect("add Site managed block");

    assert!(result.contains("127.0.0.1 lysm.test"));
    assert!(result.contains("127.0.0.1 demo.test"));
  }

  #[test]
  fn clearing_proxy_domains_preserves_sites_and_unmanaged_entries() {
    let existing = "127.0.0.1 localhost\r\n# BEGIN FABDEV MANAGED\r\n127.0.0.1 demo.test\r\n# END FABDEV MANAGED\r\n# BEGIN FABDEV PROXY MANAGED\r\n127.0.0.1 lysm.test\r\n# END FABDEV PROXY MANAGED\r\n10.0.0.2 intranet\r\n";
    let result = update_managed_block(existing, PROXY_MANAGED_START, PROXY_MANAGED_END, &[])
      .expect("clear Proxy managed block");

    assert!(result.contains("127.0.0.1 demo.test"));
    assert!(result.contains("10.0.0.2 intranet"));
    assert!(!result.contains("lysm.test"));
    assert!(!result.contains(PROXY_MANAGED_START));
  }

  #[test]
  fn accepts_only_normalized_test_domains() {
    assert!(normalize_domains(&["erp.test".to_owned(), "crm-2.test".to_owned()]).is_ok());
    assert!(normalize_domains(&["Example.test".to_owned()]).is_err());
    assert!(normalize_domains(&["example.com".to_owned()]).is_err());
  }
}
