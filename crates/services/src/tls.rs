use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fabdev_core::{normalize_domain, AppPaths, LocalCaInfo};
use rcgen::{
  BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
  KeyPair, KeyUsagePurpose,
};
use sha2::{Digest, Sha256};

const CA_COMMON_NAME: &str = "fabDev Local Development CA";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSiteCertificate {
  pub certificate: PathBuf,
  pub private_key: PathBuf,
}

pub fn ensure_local_ca(paths: &AppPaths) -> Result<LocalCaInfo> {
  let directory = tls_directory(paths);
  std::fs::create_dir_all(&directory)?;
  set_directory_permissions(&directory)?;
  let certificate_path = directory.join("ca.crt");
  let private_key_path = directory.join("ca.key");
  let fingerprint_path = directory.join("ca.sha256");

  if certificate_path.is_file() && private_key_path.is_file() && fingerprint_path.is_file() {
    let fingerprint_sha256 = std::fs::read_to_string(&fingerprint_path)?
      .trim()
      .to_owned();
    if is_sha256_fingerprint(&fingerprint_sha256) {
      return Ok(LocalCaInfo {
        certificate_path,
        fingerprint_sha256,
      });
    }
  }

  remove_if_exists(&certificate_path)?;
  remove_if_exists(&private_key_path)?;
  remove_if_exists(&fingerprint_path)?;
  let sites = directory.join("sites");
  if sites.exists() {
    std::fs::remove_dir_all(&sites)
      .context("unable to clear certificates from an incomplete CA")?;
  }

  let key_pair = KeyPair::generate().context("unable to generate local CA private key")?;
  let certificate = ca_parameters()
    .self_signed(&key_pair)
    .context("unable to generate local CA certificate")?;
  let fingerprint_sha256 = hex_digest(certificate.der().as_ref());

  write_atomic(&certificate_path, certificate.pem().as_bytes())?;
  write_private_key(&private_key_path, key_pair.serialize_pem().as_bytes())?;
  write_atomic(
    &fingerprint_path,
    format!("{fingerprint_sha256}\n").as_bytes(),
  )?;

  Ok(LocalCaInfo {
    certificate_path,
    fingerprint_sha256,
  })
}

pub fn ensure_site_certificate(paths: &AppPaths, domain: &str) -> Result<LocalSiteCertificate> {
  let domain = normalize_domain(domain).context("invalid Site domain for certificate")?;
  ensure_local_ca(paths)?;
  let directory = tls_directory(paths).join("sites");
  std::fs::create_dir_all(&directory)?;
  set_directory_permissions(&directory)?;
  let certificate = directory.join(format!("{domain}.crt"));
  let private_key = directory.join(format!("{domain}.key"));
  if certificate.is_file() && private_key.is_file() {
    return Ok(LocalSiteCertificate {
      certificate,
      private_key,
    });
  }
  remove_if_exists(&certificate)?;
  remove_if_exists(&private_key)?;

  let ca_key_path = tls_directory(paths).join("ca.key");
  let ca_key_pem = std::fs::read_to_string(&ca_key_path)
    .with_context(|| format!("unable to read local CA key: {}", ca_key_path.display()))?;
  let ca_key = KeyPair::from_pem(&ca_key_pem).context("local CA private key is invalid")?;
  let ca_certificate = ca_parameters()
    .self_signed(&ca_key)
    .context("unable to reconstruct local CA signer")?;

  let site_key = KeyPair::generate().context("unable to generate Site private key")?;
  let mut parameters = CertificateParams::new(vec![domain.clone()])?;
  let mut distinguished_name = DistinguishedName::new();
  distinguished_name.push(DnType::OrganizationName, "fabDev");
  distinguished_name.push(DnType::CommonName, domain);
  parameters.distinguished_name = distinguished_name;
  parameters.is_ca = IsCa::NoCa;
  parameters.key_usages = vec![KeyUsagePurpose::DigitalSignature];
  parameters.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
  let site_certificate = parameters
    .signed_by(&site_key, &ca_certificate, &ca_key)
    .context("unable to sign Site certificate")?;

  write_atomic(&certificate, site_certificate.pem().as_bytes())?;
  write_private_key(&private_key, site_key.serialize_pem().as_bytes())?;
  Ok(LocalSiteCertificate {
    certificate,
    private_key,
  })
}

pub fn remove_site_certificate(paths: &AppPaths, domain: &str) -> Result<()> {
  let domain = normalize_domain(domain).context("invalid Site domain for certificate removal")?;
  let directory = tls_directory(paths).join("sites");
  remove_if_exists(&directory.join(format!("{domain}.crt")))?;
  remove_if_exists(&directory.join(format!("{domain}.key")))
}

fn tls_directory(paths: &AppPaths) -> PathBuf {
  paths.config.join("tls")
}

fn ca_parameters() -> CertificateParams {
  let mut parameters = CertificateParams::default();
  let mut distinguished_name = DistinguishedName::new();
  distinguished_name.push(DnType::OrganizationName, "fabDev");
  distinguished_name.push(DnType::CommonName, CA_COMMON_NAME);
  parameters.distinguished_name = distinguished_name;
  parameters.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
  parameters.key_usages = vec![
    KeyUsagePurpose::KeyCertSign,
    KeyUsagePurpose::CrlSign,
    KeyUsagePurpose::DigitalSignature,
  ];
  parameters
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
  let pending = path.with_extension(format!(
    "{}.pending",
    path
      .extension()
      .and_then(|extension| extension.to_str())
      .unwrap_or("file")
  ));
  std::fs::write(&pending, contents).with_context(|| {
    format!(
      "unable to write certificate staging file: {}",
      pending.display()
    )
  })?;
  std::fs::rename(&pending, path)
    .with_context(|| format!("unable to activate certificate file: {}", path.display()))
}

fn write_private_key(path: &Path, contents: &[u8]) -> Result<()> {
  write_atomic(path, contents)?;
  set_private_permissions(path)
}

fn remove_if_exists(path: &Path) -> Result<()> {
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn is_sha256_fingerprint(value: &str) -> bool {
  value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex_digest(contents: &[u8]) -> String {
  let digest = Sha256::digest(contents);
  digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<()> {
  use std::os::unix::fs::PermissionsExt;

  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
  Ok(())
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> Result<()> {
  Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<()> {
  use std::os::unix::fs::PermissionsExt;

  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
  Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<()> {
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_stable_ca_and_domain_certificate() {
    let root = std::env::temp_dir().join(format!("fabdev-tls-{}", uuid::Uuid::new_v4()));
    let paths = AppPaths::from_root(&root);
    paths.ensure().expect("create app paths");

    let first_ca = ensure_local_ca(&paths).expect("create local CA");
    let second_ca = ensure_local_ca(&paths).expect("reuse local CA");
    assert_eq!(first_ca, second_ca);
    assert!(first_ca.certificate_path.is_file());
    assert_eq!(first_ca.fingerprint_sha256.len(), 64);

    let certificate = ensure_site_certificate(&paths, "erp.test").expect("create Site cert");
    let first_contents = std::fs::read(&certificate.certificate).expect("read Site cert");
    let reused = ensure_site_certificate(&paths, "erp.test").expect("reuse Site cert");
    assert_eq!(certificate, reused);
    assert_eq!(
      first_contents,
      std::fs::read(&reused.certificate).expect("read reused Site cert")
    );
    assert!(std::fs::read_to_string(&certificate.certificate)
      .expect("read PEM")
      .contains("BEGIN CERTIFICATE"));
    assert!(std::fs::read_to_string(&certificate.private_key)
      .expect("read private key")
      .contains("BEGIN PRIVATE KEY"));

    remove_site_certificate(&paths, "erp.test").expect("remove Site cert");
    assert!(!certificate.certificate.exists());
    assert!(!certificate.private_key.exists());
    std::fs::remove_dir_all(root).expect("remove TLS fixture");
  }

  #[test]
  fn rejects_certificate_paths_outside_test_domains() {
    let paths = AppPaths::from_root("/tmp/fabdev-invalid-tls");
    assert!(ensure_site_certificate(&paths, "example.com").is_err());
    assert!(remove_site_certificate(&paths, "../escape.test").is_err());
  }

  #[test]
  fn recognizes_only_full_sha256_fingerprints() {
    assert!(is_sha256_fingerprint(&"a".repeat(64)));
    assert!(!is_sha256_fingerprint("abc"));
    assert!(!is_sha256_fingerprint(&"g".repeat(64)));
  }
}
