use std::collections::HashSet;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty};
use hyper::body::Incoming;
use hyper::header::HOST;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinSet;

type ResponseBody = BoxBody<Bytes, hyper::Error>;

pub(super) async fn proxy(
  client: TcpStream,
  upstream: TcpStream,
  initial_request: Vec<u8>,
  allowed_domains: Arc<RwLock<Option<HashSet<String>>>>,
) -> Result<()> {
  let (sender, connection) = hyper::client::conn::http1::Builder::new()
    .preserve_header_case(true)
    .title_case_headers(true)
    .handshake(TokioIo::new(upstream))
    .await?;
  // Keep the upstream driver owned by this connection so Stop and Drop cancel it too.
  let mut tasks = JoinSet::new();
  tasks.spawn(connection.with_upgrades());
  let sender = Arc::new(Mutex::new(sender));
  let upgrades = Arc::new(Mutex::new(None));
  let pending_upgrades = Arc::clone(&upgrades);
  let service = service_fn(move |mut request: Request<Incoming>| {
    let sender = Arc::clone(&sender);
    let allowed_domains = Arc::clone(&allowed_domains);
    let upgrades = Arc::clone(&pending_upgrades);
    async move {
      let allowed = {
        let domains = allowed_domains.read().await;
        domains
          .as_ref()
          .is_some_and(|domains| request_is_allowed(&request, domains))
      };
      if !allowed {
        return Ok::<_, Infallible>(empty_response(StatusCode::FORBIDDEN));
      }
      // Nginx must route using the same Host that passed the current share policy.
      if let Some(path) = request.uri().path_and_query().cloned() {
        *request.uri_mut() = path.into();
      }
      let client_upgrade = hyper::upgrade::on(&mut request);
      let response = match sender.lock().await.send_request(request).await {
        Ok(mut response) => {
          if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let upstream_upgrade = hyper::upgrade::on(&mut response);
            *upgrades.lock().await = Some((client_upgrade, upstream_upgrade));
          }
          response.map(BodyExt::boxed)
        }
        Err(error) => {
          eprintln!("fabDev Share upstream request failed: {error}");
          empty_response(StatusCode::BAD_GATEWAY)
        }
      };
      Ok(response)
    }
  });
  hyper::server::conn::http1::Builder::new()
    .preserve_header_case(true)
    .title_case_headers(true)
    .max_buf_size(super::MAX_HTTP_HEADER_SIZE)
    .serve_connection(
      TokioIo::new(PrefixedStream {
        prefix: initial_request.into(),
        stream: client,
      }),
      service,
    )
    .with_upgrades()
    .await?;

  // Preserve existing WebSocket and other HTTP Upgrade traffic for the allowed Site.
  let upgraded = upgrades.lock().await.take();
  if let Some((client, upstream)) = upgraded {
    let (client, upstream) = tokio::try_join!(client, upstream)?;
    copy_bidirectional(&mut TokioIo::new(client), &mut TokioIo::new(upstream)).await?;
  }
  tasks.shutdown().await;
  Ok(())
}

fn request_is_allowed(request: &Request<Incoming>, domains: &HashSet<String>) -> bool {
  if request.method() == Method::CONNECT || request.headers().get_all(HOST).iter().count() != 1 {
    return false;
  }
  let Some(host) = request
    .headers()
    .get(HOST)
    .and_then(|host| host.to_str().ok())
    .and_then(|host| host.parse::<hyper::http::uri::Authority>().ok())
  else {
    return false;
  };
  if !domains.contains(&host.host().to_ascii_lowercase()) {
    return false;
  }
  request
    .uri()
    .authority()
    .is_none_or(|authority| authority.as_str().eq_ignore_ascii_case(host.as_str()))
}

fn empty_response(status: StatusCode) -> Response<ResponseBody> {
  let mut response = Response::new(
    Empty::<Bytes>::new()
      .map_err(|error| match error {})
      .boxed(),
  );
  *response.status_mut() = status;
  response
}

struct PrefixedStream {
  prefix: Bytes,
  stream: TcpStream,
}

impl AsyncRead for PrefixedStream {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buffer: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    let this = self.get_mut();
    if !this.prefix.is_empty() {
      let count = buffer.remaining().min(this.prefix.len());
      buffer.put_slice(&this.prefix.split_to(count));
      Poll::Ready(Ok(()))
    } else {
      Pin::new(&mut this.stream).poll_read(cx, buffer)
    }
  }
}

impl AsyncWrite for PrefixedStream {
  fn poll_write(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buffer: &[u8],
  ) -> Poll<std::io::Result<usize>> {
    Pin::new(&mut self.get_mut().stream).poll_write(cx, buffer)
  }

  fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.get_mut().stream).poll_flush(cx)
  }

  fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
  }
}
