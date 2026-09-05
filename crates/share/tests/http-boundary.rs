use std::time::Duration;

use fabdev_share::ShareServer;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::timeout;

async fn read_headers(stream: &mut BufReader<TcpStream>) -> String {
  timeout(Duration::from_secs(2), async {
    let mut headers = String::new();
    loop {
      let mut line = String::new();
      if stream.read_line(&mut line).await.unwrap() == 0 {
        break;
      }
      headers.push_str(&line);
      if line == "\r\n" {
        break;
      }
    }
    headers
  })
  .await
  .expect("HTTP headers must arrive")
}

async fn fixture() -> (ShareServer, BufReader<TcpStream>, JoinSet<()>) {
  let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let share = ShareServer::start_restricted(
    "127.0.0.1:0".parse().unwrap(),
    upstream.local_addr().unwrap(),
    vec!["demo.test".to_owned()],
  )
  .await
  .unwrap();
  let mut tasks = JoinSet::new();
  tasks.spawn(async move {
    let (stream, _) = upstream.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    while !read_headers(&mut stream).await.is_empty() {
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    }
  });
  let client = BufReader::new(TcpStream::connect(share.local_addr()).await.unwrap());
  (share, client, tasks)
}

#[tokio::test]
async fn preserves_allowed_keep_alive_and_absolute_requests() {
  let (mut share, mut client, _tasks) = fixture().await;
  for path in ["/", "/next", "http://demo.test/last"] {
    client
      .write_all(format!("GET {path} HTTP/1.1\r\nHost: demo.test\r\n\r\n").as_bytes())
      .await
      .unwrap();
    assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 200"));
  }
  share.stop().await.unwrap();
}

#[tokio::test]
async fn checks_every_pipelined_request_host() {
  let (mut share, mut client, _tasks) = fixture().await;
  client
    .write_all(
      b"GET / HTTP/1.1\r\nHost: demo.test\r\n\r\nGET / HTTP/1.1\r\nHost: private.test\r\n\r\n",
    )
    .await
    .unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 200"));
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 403"));
  share.stop().await.unwrap();
}

#[tokio::test]
async fn rechecks_changed_domains_on_keep_alive_connections() {
  let (mut share, mut client, _tasks) = fixture().await;
  let request = b"GET / HTTP/1.1\r\nHost: demo.test\r\n\r\n";
  client.write_all(request).await.unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 200"));
  share
    .set_allowed_domains(vec!["other.test".to_owned()])
    .await
    .unwrap();
  client.write_all(request).await.unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 403"));
  share.stop().await.unwrap();
}

#[tokio::test]
async fn rejects_an_absolute_request_target_for_an_unshared_site() {
  let (mut share, mut client, _tasks) = fixture().await;
  client
    .write_all(b"GET http://private.test/ HTTP/1.1\r\nHost: demo.test\r\n\r\n")
    .await
    .unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 403"));
  share.stop().await.unwrap();
}

#[tokio::test]
async fn preserves_streamed_post_requests_and_responses() {
  let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let mut share = ShareServer::start_restricted(
    "127.0.0.1:0".parse().unwrap(),
    upstream.local_addr().unwrap(),
    vec!["demo.test".to_owned()],
  )
  .await
  .unwrap();
  let (request_started, request_received) = oneshot::channel();
  let (response_received, finish_response) = oneshot::channel();
  let mut tasks = JoinSet::new();
  tasks.spawn(async move {
    let (stream, _) = upstream.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    let headers = read_headers(&mut stream).await;
    assert!(headers.starts_with("POST /upload HTTP/1.1"));
    assert!(headers.contains("Host: demo.test"));
    let mut chunk = [0; 4];
    stream.read_exact(&mut chunk).await.unwrap();
    assert_eq!(&chunk, b"ping");
    request_started.send(()).unwrap();
    stream.read_exact(&mut chunk).await.unwrap();
    assert_eq!(&chunk, b"pong");
    stream
      .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nping")
      .await
      .unwrap();
    finish_response.await.unwrap();
    stream.write_all(b"pong").await.unwrap();
  });
  let mut client = BufReader::new(TcpStream::connect(share.local_addr()).await.unwrap());
  client
    .write_all(b"POST /upload HTTP/1.1\r\nHost: demo.test\r\nContent-Length: 8\r\n\r\nping")
    .await
    .unwrap();
  timeout(Duration::from_secs(2), request_received)
    .await
    .expect("the first POST bytes must reach upstream before upload completion")
    .unwrap();
  client.write_all(b"pong").await.unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 200"));
  let mut chunk = [0; 4];
  timeout(Duration::from_secs(2), client.read_exact(&mut chunk))
    .await
    .expect("the first response bytes must arrive before response completion")
    .unwrap();
  assert_eq!(&chunk, b"ping");
  response_received.send(()).unwrap();
  timeout(Duration::from_secs(2), client.read_exact(&mut chunk))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(&chunk, b"pong");
  tasks.join_next().await.unwrap().unwrap();
  share.stop().await.unwrap();
}

#[tokio::test]
async fn preserves_http_upgrade_and_closes_the_tunnel_on_stop() {
  let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let mut share = ShareServer::start_restricted(
    "127.0.0.1:0".parse().unwrap(),
    upstream.local_addr().unwrap(),
    vec!["demo.test".to_owned()],
  )
  .await
  .unwrap();
  let mut tasks = JoinSet::new();
  tasks.spawn(async move {
    let (stream, _) = upstream.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    assert!(read_headers(&mut stream)
      .await
      .contains("Upgrade: websocket"));
    stream
      .write_all(
        b"HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n",
      )
      .await
      .unwrap();
    let mut request = [0; 4];
    stream.read_exact(&mut request).await.unwrap();
    assert_eq!(&request, b"ping");
    stream.write_all(b"pong").await.unwrap();
    let mut byte = [0];
    let read = timeout(Duration::from_secs(2), stream.read(&mut byte))
      .await
      .expect("Stop must close the upstream tunnel");
    assert!(matches!(read, Ok(0)) || read.is_err());
  });
  let mut client = BufReader::new(TcpStream::connect(share.local_addr()).await.unwrap());
  client
    .write_all(
      b"GET /ws HTTP/1.1\r\nHost: demo.test\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n",
    )
    .await
    .unwrap();
  assert!(read_headers(&mut client).await.starts_with("HTTP/1.1 101"));
  client.write_all(b"ping").await.unwrap();
  let mut response = [0; 4];
  timeout(Duration::from_secs(2), client.read_exact(&mut response))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(&response, b"pong");
  share.stop().await.unwrap();
  let read = timeout(Duration::from_secs(2), client.read(&mut response))
    .await
    .expect("Stop must close the client tunnel");
  assert!(matches!(read, Ok(0)) || read.is_err());
  tasks.join_next().await.unwrap().unwrap();
}
