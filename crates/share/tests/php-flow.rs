use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use fabdev_share::ShareServer;
use http_body_util::{BodyExt, Empty};
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

struct PhpFixture {
  child: Child,
  root: PathBuf,
}

impl Drop for PhpFixture {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
    let _ = std::fs::remove_dir_all(&self.root);
  }
}

#[tokio::test]
#[ignore = "requires FABDEV_SHARE_TEST_PHP pointing to an existing PHP CLI binary"]
async fn serves_real_php_and_releases_all_test_listeners() {
  let php = std::env::var_os("FABDEV_SHARE_TEST_PHP").expect("provide an existing PHP binary");
  let root = std::env::temp_dir().join(format!(
    "fabdev-share-php-{}-{}",
    std::process::id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  std::fs::create_dir_all(&root).unwrap();
  std::fs::write(
    root.join("index.php"),
    "<?php echo 'fabdev-share-php-' . (20 + 22);",
  )
  .unwrap();
  let reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let upstream = reservation.local_addr().unwrap();
  drop(reservation);
  let log = std::fs::File::create(root.join("php.log")).unwrap();
  let child = Command::new(php)
    .args(["-n", "-S", &upstream.to_string(), "-t"])
    .arg(&root)
    .stdout(Stdio::null())
    .stderr(log)
    .spawn()
    .expect("start isolated PHP server");
  let mut fixture = PhpFixture { child, root };
  timeout(Duration::from_secs(5), async {
    loop {
      assert!(
        fixture.child.try_wait().unwrap().is_none(),
        "PHP exited early"
      );
      if TcpStream::connect(upstream).await.is_ok() {
        break;
      }
      sleep(Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("PHP must become ready");

  let mut share = ShareServer::start_restricted(
    "127.0.0.1:0".parse().unwrap(),
    upstream,
    vec!["demo.test".to_owned()],
  )
  .await
  .unwrap();
  let address = share.local_addr();
  for (host, status) in [("demo.test", 200), ("private.test", 403)] {
    let client = TcpStream::connect(address).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(client))
      .await
      .unwrap();
    let mut tasks = JoinSet::new();
    tasks.spawn(connection);
    let request = Request::builder()
      .uri("/")
      .header("Host", host)
      .header("Connection", "close")
      .body(Empty::<Bytes>::new())
      .unwrap();
    let body = timeout(Duration::from_secs(5), async {
      let response = sender.send_request(request).await.unwrap();
      assert_eq!(response.status().as_u16(), status);
      response.into_body().collect().await.unwrap().to_bytes()
    })
    .await
    .unwrap();
    if status == 200 {
      assert_eq!(&body[..], b"fabdev-share-php-42");
    } else {
      assert!(body.is_empty());
    }
    tasks.shutdown().await;
  }
  share.stop().await.unwrap();
  let root = fixture.root.clone();
  drop(fixture);
  assert!(!root.exists(), "the isolated PHP fixture must be removed");
  TcpListener::bind(address)
    .await
    .expect("share port released");
  TcpListener::bind(upstream)
    .await
    .expect("PHP port released");
}
