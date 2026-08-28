use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use fabdev_core::Site;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct NginxSiteConfig {
  pub site: Site,
  pub nginx_root: PathBuf,
  pub fastcgi_endpoint: Option<FastCgiEndpoint>,
  pub listen_port: u16,
  pub https_listen_port: u16,
  pub tls: Option<NginxTlsConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NginxTlsConfig {
  pub certificate: PathBuf,
  pub private_key: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FastCgiEndpoint {
  UnixSocket(PathBuf),
  Tcp(SocketAddr),
}

#[derive(Debug, Error)]
pub enum SiteDriverError {
  #[error("PHP-FPM socket must be an absolute path")]
  RelativeSocket,
  #[error("Nginx Runtime root must be an absolute path")]
  RelativeNginxRoot,
  #[error("Nginx listen port must not be zero")]
  ZeroPort,
  #[error("Nginx HTTPS listen port must not be zero")]
  ZeroHttpsPort,
  #[error("TLS certificate and private key paths must be absolute")]
  RelativeTlsPath,
}

pub fn render_nginx_site(config: &NginxSiteConfig) -> Result<String, SiteDriverError> {
  if let Some(FastCgiEndpoint::UnixSocket(socket)) = &config.fastcgi_endpoint {
    if !socket.is_absolute() {
      return Err(SiteDriverError::RelativeSocket);
    }
  }
  if !config.nginx_root.is_absolute() {
    return Err(SiteDriverError::RelativeNginxRoot);
  }
  if config.listen_port == 0 {
    return Err(SiteDriverError::ZeroPort);
  }
  if let Some(tls) = &config.tls {
    if config.https_listen_port == 0 {
      return Err(SiteDriverError::ZeroHttpsPort);
    }
    if !tls.certificate.is_absolute() || !tls.private_key.is_absolute() {
      return Err(SiteDriverError::RelativeTlsPath);
    }
  }
  let root = quote_path(&config.site.document_root);
  let (index, fallback, php_location, php_status_location) = match &config.fastcgi_endpoint {
    Some(endpoint) => {
      let fastcgi_params = quote_path(&config.nginx_root.join("conf/fastcgi_params"));
      let fastcgi_pass = match endpoint {
        FastCgiEndpoint::UnixSocket(socket) => {
          format!("\"unix:{}\"", socket.to_string_lossy())
        }
        FastCgiEndpoint::Tcp(address) => address.to_string(),
      };
      (
        "index.php index.html",
        "/index.php?$query_string",
        format!(
          r#"
  location ~ \.php$ {{
    try_files $uri =404;
    include {fastcgi_params};
    fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    fastcgi_pass {fastcgi_pass};
  }}
"#,
        ),
        format!(
          r#"
  location = /__fabdev/php-fpm-status {{
    access_log off;
    allow 127.0.0.1;
    deny all;
    include {fastcgi_params};
    fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    fastcgi_pass {fastcgi_pass};
  }}
"#,
        ),
      )
    }
    None => ("index.html", "=404", String::new(), String::new()),
  };
  let application_server = format!(
    r#"server {{
  listen 127.0.0.1:{listen_port};
  server_name {domain};
  root {root};
  index {index};

  location / {{
    try_files $uri $uri/ {fallback};
  }}
{php_location}
{php_status_location}

  location ~ /\. {{
    deny all;
  }}
}}
"#,
    domain = config.site.domain,
    listen_port = config.listen_port,
  );
  let Some(tls) = &config.tls else {
    return Ok(application_server);
  };
  let certificate = quote_path(&tls.certificate);
  let private_key = quote_path(&tls.private_key);
  Ok(format!(
    r#"server {{
  listen 127.0.0.1:{listen_port};
  server_name {domain};
  location / {{
    return 301 https://$host$request_uri;
  }}
{php_status_location}
}}

server {{
  listen 127.0.0.1:{https_listen_port} ssl;
  http2 on;
  server_name {domain};
  ssl_certificate {certificate};
  ssl_certificate_key {private_key};
  ssl_protocols TLSv1.2 TLSv1.3;
  root {root};
  index {index};

  location / {{
    try_files $uri $uri/ {fallback};
  }}
{php_location}
{php_status_location}

  location ~ /\. {{
    deny all;
  }}
}}
"#,
    domain = config.site.domain,
    listen_port = config.listen_port,
    https_listen_port = config.https_listen_port,
  ))
}

fn quote_path(path: &Path) -> String {
  let value = path.to_string_lossy();
  let value = if let Some(path) = value.strip_prefix(r"\\?\UNC\") {
    format!(r"\\{path}")
  } else if let Some(path) = value.strip_prefix(r"\\?\") {
    path.to_owned()
  } else {
    value.into_owned()
  };
  let value = value.replace('\\', "/").replace('"', "\\\"");
  format!("\"{value}\"")
}

#[cfg(test)]
mod tests {
  use fabdev_core::{PhpVersion, Site};
  use uuid::Uuid;

  use super::*;

  fn site() -> Site {
    Site {
      id: Uuid::new_v4(),
      name: "ERP Demo".to_owned(),
      domain: "erp-demo.test".to_owned(),
      project_path: "/tmp/ERP Demo".into(),
      document_root: "/tmp/ERP Demo/public".into(),
      php_version: Some(PhpVersion { major: 8, minor: 2 }),
      enabled: true,
      secured: false,
    }
  }

  #[test]
  fn renders_site_for_unix_socket() {
    let output = render_nginx_site(&NginxSiteConfig {
      site: site(),
      nginx_root: "/tmp/fabdev/nginx".into(),
      fastcgi_endpoint: Some(FastCgiEndpoint::UnixSocket("/tmp/fabdev/php82.sock".into())),
      listen_port: 8080,
      https_listen_port: 8443,
      tls: None,
    })
    .expect("render config");
    assert!(output.contains("server_name erp-demo.test;"));
    assert!(output.contains("listen 127.0.0.1:8080;"));
    assert!(output.contains("root \"/tmp/ERP Demo/public\";"));
    assert!(output.contains("fastcgi_pass \"unix:/tmp/fabdev/php82.sock\";"));
    assert!(output.contains("include \"/tmp/fabdev/nginx/conf/fastcgi_params\";"));
    assert!(output.contains("location = /__fabdev/php-fpm-status"));
    assert!(output.contains("allow 127.0.0.1;"));
    assert!(output.contains("access_log off;"));
    assert_eq!(
      output
        .matches("fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;")
        .count(),
      2
    );
  }

  #[test]
  fn quotes_php_socket_with_spaces() {
    let output = render_nginx_site(&NginxSiteConfig {
      site: site(),
      nginx_root: "/tmp/Application Support/fabdev/nginx".into(),
      fastcgi_endpoint: Some(FastCgiEndpoint::UnixSocket(
        "/tmp/Application Support/fabdev/php82.sock".into(),
      )),
      listen_port: 8080,
      https_listen_port: 8443,
      tls: None,
    })
    .expect("render config");
    assert!(output.contains("fastcgi_pass \"unix:/tmp/Application Support/fabdev/php82.sock\";"));
  }

  #[test]
  fn quotes_windows_verbatim_path_for_nginx() {
    assert_eq!(
      quote_path(Path::new(r"\\?\C:\Users\dev\ERP Demo\public")),
      r#""C:/Users/dev/ERP Demo/public""#
    );
    assert_eq!(
      quote_path(Path::new(r"\\?\UNC\server\share\public")),
      r#""//server/share/public""#
    );
  }

  #[test]
  fn renders_site_for_tcp_fastcgi() {
    let output = render_nginx_site(&NginxSiteConfig {
      site: site(),
      nginx_root: "/tmp/fabdev/nginx".into(),
      fastcgi_endpoint: Some(FastCgiEndpoint::Tcp(
        "127.0.0.1:19082".parse().expect("parse address"),
      )),
      listen_port: 80,
      https_listen_port: 443,
      tls: None,
    })
    .expect("render config");

    assert!(output.contains("listen 127.0.0.1:80;"));
    assert!(output.contains("fastcgi_pass 127.0.0.1:19082;"));
  }

  #[test]
  fn renders_static_site_without_php_location() {
    let mut static_site = site();
    static_site.php_version = None;
    let output = render_nginx_site(&NginxSiteConfig {
      site: static_site,
      nginx_root: "/tmp/fabdev/nginx".into(),
      fastcgi_endpoint: None,
      listen_port: 8080,
      https_listen_port: 8443,
      tls: None,
    })
    .expect("render static config");

    assert!(output.contains("index index.html;"));
    assert!(output.contains("try_files $uri $uri/ =404;"));
    assert!(!output.contains("fastcgi_pass"));
    assert!(!output.contains("location ~ \\.php$"));
    assert!(!output.contains("/__fabdev/php-fpm-status"));
  }

  #[test]
  fn renders_https_redirect_and_tls_server() {
    let mut secured_site = site();
    secured_site.secured = true;
    let output = render_nginx_site(&NginxSiteConfig {
      site: secured_site,
      nginx_root: "/tmp/fabdev/nginx".into(),
      fastcgi_endpoint: Some(FastCgiEndpoint::UnixSocket("/tmp/fabdev/php82.sock".into())),
      listen_port: 8080,
      https_listen_port: 8443,
      tls: Some(NginxTlsConfig {
        certificate: "/tmp/fabdev/tls/erp.test.crt".into(),
        private_key: "/tmp/fabdev/tls/erp.test.key".into(),
      }),
    })
    .expect("render secure config");

    assert!(output.contains("return 301 https://$host$request_uri;"));
    assert_eq!(
      output
        .matches("location = /__fabdev/php-fpm-status")
        .count(),
      2
    );
    assert!(output.contains("listen 127.0.0.1:8443 ssl;"));
    assert!(output.contains("http2 on;"));
    assert!(output.contains("ssl_certificate \"/tmp/fabdev/tls/erp.test.crt\";"));
    assert!(output.contains("ssl_certificate_key \"/tmp/fabdev/tls/erp.test.key\";"));
  }
}
