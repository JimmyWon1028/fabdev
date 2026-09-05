use std::time::Duration;

use fabdev_share::ShareServer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

async fn assert_connections_close(drop_server: bool) {
  let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let mut share = ShareServer::start(
    "127.0.0.1:0".parse().unwrap(),
    upstream.local_addr().unwrap(),
  )
  .await
  .unwrap();
  let address = share.local_addr();
  let mut client = TcpStream::connect(address).await.unwrap();
  let (mut server, _) = timeout(Duration::from_secs(2), upstream.accept())
    .await
    .unwrap()
    .unwrap();
  client.write_all(b"ping").await.unwrap();
  let mut request = [0; 4];
  server.read_exact(&mut request).await.unwrap();
  assert_eq!(&request, b"ping");

  if drop_server {
    drop(share);
  } else {
    share.stop().await.unwrap();
  }
  for stream in [&mut client, &mut server] {
    let mut byte = [0];
    let read = timeout(Duration::from_secs(2), stream.read(&mut byte))
      .await
      .expect("stopping LAN Share must close established connections");
    assert!(matches!(read, Ok(0)) || read.is_err());
  }
  TcpListener::bind(address)
    .await
    .expect("the listener must be released");
}

#[tokio::test]
async fn stop_closes_established_connections() {
  assert_connections_close(false).await;
}

#[tokio::test]
async fn drop_closes_established_connections() {
  assert_connections_close(true).await;
}
