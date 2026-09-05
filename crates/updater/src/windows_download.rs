use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context};
use futures_util::{stream, StreamExt};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DEFAULT_SEGMENT_SIZE: u64 = 8 * 1024 * 1024;
const MAX_PARALLEL_SEGMENTS: usize = 4;
const MAX_DOWNLOAD_ATTEMPTS: usize = 4;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Eq, PartialEq)]
struct DownloadSegment {
  start: u64,
  end: u64,
  path: PathBuf,
}

pub(crate) struct WindowsArtifactDownload<'a> {
  pub client: &'a Client,
  pub url: &'a str,
  pub size: u64,
  pub sha256: &'a str,
  pub partial: &'a Path,
  pub target: &'a Path,
}

impl DownloadSegment {
  fn length(&self) -> u64 {
    self.end - self.start + 1
  }
}

pub(crate) async fn download_windows_artifact<F, C>(
  request: WindowsArtifactDownload<'_>,
  on_progress: &mut F,
  is_cancelled: &C,
) -> anyhow::Result<()>
where
  F: FnMut(u64, u64),
  C: Fn() -> bool,
{
  download_windows_artifact_with_segment_size(
    request.client,
    request.url,
    request.size,
    request.sha256,
    request.partial,
    request.target,
    DEFAULT_SEGMENT_SIZE,
    on_progress,
    is_cancelled,
  )
  .await
}

#[allow(clippy::too_many_arguments)]
async fn download_windows_artifact_with_segment_size<F, C>(
  client: &Client,
  url: &str,
  size: u64,
  sha256: &str,
  partial: &Path,
  target: &Path,
  segment_size: u64,
  on_progress: &mut F,
  is_cancelled: &C,
) -> anyhow::Result<()>
where
  F: FnMut(u64, u64),
  C: Fn() -> bool,
{
  if size == 0 || segment_size == 0 {
    bail!("Windows update artifact has an invalid size");
  }
  let segments = build_segments(partial, size, segment_size)?;
  let result = crate::cancellation::with_cancellation(
    async {
      if is_cancelled() {
        remove_file_if_exists(partial).await?;
        cleanup_segments(&segments).await;
        bail!("Windows update download was cancelled");
      }

      remove_file_if_exists(partial).await?;
      let mut resumed = 0_u64;
      let mut pending = Vec::new();
      for segment in &segments {
        match tokio::fs::metadata(&segment.path).await {
          Ok(metadata) if metadata.is_file() && metadata.len() == segment.length() => {
            resumed += segment.length();
          }
          Ok(_) => {
            remove_file_if_exists(&segment.path).await?;
            pending.push(segment.clone());
          }
          Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            pending.push(segment.clone());
          }
          Err(error) => return Err(error.into()),
        }
      }
      on_progress(resumed, size);

      if !pending.is_empty() {
        verify_range_support(client, url, size).await?;
      }

      let downloads = stream::iter(pending.into_iter().map(|segment| async move {
        download_segment(client, url, size, &segment, is_cancelled).await
      }))
      .buffer_unordered(MAX_PARALLEL_SEGMENTS);
      tokio::pin!(downloads);

      let mut downloaded = resumed;
      while let Some(result) = downloads.next().await {
        match result {
          Ok(count) => {
            downloaded = downloaded
              .checked_add(count)
              .context("Windows update download size overflow")?;
            on_progress(downloaded, size);
          }
          Err(error) => {
            if is_cancelled() {
              cleanup_segments(&segments).await;
              bail!("Windows update download was cancelled");
            }
            return Err(error);
          }
        }
      }

      if is_cancelled() {
        cleanup_segments(&segments).await;
        bail!("Windows update download was cancelled");
      }
      if downloaded != size {
        bail!("Windows update download is incomplete");
      }

      Ok(())
    },
    is_cancelled,
    "Windows update download was cancelled",
  )
  .await;
  // The download future is dropped before cleanup, closing active segment handles.
  if result.is_err() && is_cancelled() {
    remove_file_if_exists(partial).await?;
    cleanup_segments(&segments).await;
  }
  result?;
  let checksum = combine_and_hash_segments(&segments, partial).await?;
  if checksum != sha256 {
    remove_file_if_exists(partial).await?;
    cleanup_segments(&segments).await;
    bail!("Windows update artifact SHA-256 does not match");
  }
  tokio::fs::rename(partial, target)
    .await
    .context("unable to finalize the verified Windows update artifact")?;
  cleanup_segments(&segments).await;
  Ok(())
}

fn build_segments(
  partial: &Path,
  size: u64,
  segment_size: u64,
) -> anyhow::Result<Vec<DownloadSegment>> {
  let file_name = partial
    .file_name()
    .and_then(|name| name.to_str())
    .context("Windows update partial path has no UTF-8 file name")?;
  let parent = partial
    .parent()
    .context("Windows update partial path has no parent directory")?;
  let mut segments = Vec::new();
  let mut start = 0_u64;
  let mut index = 0_usize;
  while start < size {
    let end = start.saturating_add(segment_size - 1).min(size - 1);
    segments.push(DownloadSegment {
      start,
      end,
      path: parent.join(format!("{file_name}.{index:04}.resume")),
    });
    start = end + 1;
    index += 1;
  }
  Ok(segments)
}

async fn verify_range_support(client: &Client, url: &str, size: u64) -> anyhow::Result<()> {
  let expected = format!("bytes 0-0/{size}");
  let mut last_error = None;
  for attempt in 0..MAX_DOWNLOAD_ATTEMPTS {
    match client.get(url).header(RANGE, "bytes=0-0").send().await {
      Ok(response)
        if response.status() == StatusCode::PARTIAL_CONTENT
          && response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            == Some(expected.as_str()) =>
      {
        return Ok(())
      }
      Ok(response) => {
        last_error = Some(anyhow::anyhow!(
          "Windows update server does not support validated byte ranges: HTTP {}",
          response.status()
        ));
      }
      Err(error) => {
        last_error = Some(error.into());
      }
    }
    retry_delay(attempt).await;
  }
  Err(last_error.expect("range support attempts always produce a result"))
}

async fn download_segment<C>(
  client: &Client,
  url: &str,
  total_size: u64,
  segment: &DownloadSegment,
  is_cancelled: &C,
) -> anyhow::Result<u64>
where
  C: Fn() -> bool,
{
  let expected_range = format!("bytes {}-{}/{total_size}", segment.start, segment.end);
  let request_range = format!("bytes={}-{}", segment.start, segment.end);
  let mut last_error = None;

  for attempt in 0..MAX_DOWNLOAD_ATTEMPTS {
    if is_cancelled() {
      remove_file_if_exists(&segment.path).await?;
      bail!("Windows update download was cancelled");
    }
    let result = download_segment_once(
      client,
      url,
      segment,
      &request_range,
      &expected_range,
      is_cancelled,
    )
    .await;
    match result {
      Ok(()) => return Ok(segment.length()),
      Err(error) => {
        last_error = Some(error);
        remove_file_if_exists(&segment.path).await?;
      }
    }
    retry_delay(attempt).await;
  }

  Err(last_error.expect("segment attempts always produce a result"))
}

async fn download_segment_once<C>(
  client: &Client,
  url: &str,
  segment: &DownloadSegment,
  request_range: &str,
  expected_range: &str,
  is_cancelled: &C,
) -> anyhow::Result<()>
where
  C: Fn() -> bool,
{
  let response = client
    .get(url)
    .header(RANGE, request_range)
    .send()
    .await
    .context("unable to download a Windows update segment")?;
  if response.status() != StatusCode::PARTIAL_CONTENT
    || response
      .headers()
      .get(CONTENT_RANGE)
      .and_then(|value| value.to_str().ok())
      != Some(expected_range)
  {
    bail!(
      "Windows update segment returned an invalid range response: HTTP {}",
      response.status()
    );
  }
  if response
    .content_length()
    .is_some_and(|length| length != segment.length())
  {
    bail!("Windows update segment size does not match the requested range");
  }

  let mut file = tokio::fs::File::create(&segment.path)
    .await
    .context("unable to create a Windows update segment file")?;
  let mut downloaded = 0_u64;
  let mut body = response.bytes_stream();
  while let Some(chunk) = body.next().await {
    if is_cancelled() {
      bail!("Windows update download was cancelled");
    }
    let chunk = chunk.context("unable to read a Windows update segment")?;
    downloaded = downloaded
      .checked_add(chunk.len() as u64)
      .context("Windows update segment size overflow")?;
    if downloaded > segment.length() {
      bail!("Windows update segment exceeds the requested range");
    }
    file
      .write_all(&chunk)
      .await
      .context("unable to write a Windows update segment")?;
  }
  file
    .flush()
    .await
    .context("unable to flush a Windows update segment")?;
  file
    .sync_all()
    .await
    .context("unable to sync a Windows update segment")?;
  if downloaded != segment.length() {
    bail!("Windows update segment download is incomplete");
  }
  Ok(())
}

async fn combine_and_hash_segments(
  segments: &[DownloadSegment],
  partial: &Path,
) -> anyhow::Result<String> {
  let mut output = tokio::fs::File::create(partial)
    .await
    .context("unable to create the combined Windows update artifact")?;
  let mut hasher = Sha256::new();
  let mut buffer = vec![0_u8; 64 * 1024];
  for segment in segments {
    let mut input = tokio::fs::File::open(&segment.path)
      .await
      .context("unable to open a completed Windows update segment")?;
    loop {
      let count = input
        .read(&mut buffer)
        .await
        .context("unable to read a completed Windows update segment")?;
      if count == 0 {
        break;
      }
      output
        .write_all(&buffer[..count])
        .await
        .context("unable to combine Windows update segments")?;
      hasher.update(&buffer[..count]);
    }
  }
  output
    .flush()
    .await
    .context("unable to flush the combined Windows update artifact")?;
  output
    .sync_all()
    .await
    .context("unable to sync the combined Windows update artifact")?;
  Ok(hex::encode(hasher.finalize()))
}

async fn cleanup_segments(segments: &[DownloadSegment]) {
  for segment in segments {
    let _ = remove_file_if_exists(&segment.path).await;
  }
}

async fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
  match tokio::fs::remove_file(path).await {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

async fn retry_delay(attempt: usize) {
  if attempt + 1 < MAX_DOWNLOAD_ATTEMPTS {
    let multiplier = 1_u32 << attempt.min(4);
    tokio::time::sleep(RETRY_BASE_DELAY * multiplier).await;
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use super::*;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use uuid::Uuid;

  async fn assert_cancels_stalled_download(stage: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    let root = std::env::temp_dir().join(format!("fabdev-stalled-download-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let partial = root.join("fixture.part");
    let target = root.join("fixture.exe");
    let segments = build_segments(&partial, 16, 8).unwrap();
    std::fs::write(&segments[0].path, b"complete").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (ready, reached) = oneshot::channel();
    let body_stalled = stage == "body";
    let stage = stage.to_owned();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut request = [0; 2048];
      assert!(socket.read(&mut request).await.unwrap() > 0);
      if stage != "range" {
        socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/16\r\nConnection: close\r\n\r\nc").await.unwrap();
        drop(socket);
        let (next, _) = listener.accept().await.unwrap();
        socket = next;
        assert!(socket.read(&mut request).await.unwrap() > 0);
        if stage == "body" {
          socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 8\r\nContent-Range: bytes 8-15/16\r\n\r\nx").await.unwrap();
        }
      }
      ready.send(()).unwrap();
      let mut remainder = Vec::new();
      socket.read_to_end(&mut remainder).await.unwrap();
      drop(socket);
      if stage == "body" {
        let payload = b"0123456789abcdef";
        for _ in 0..3 {
          let (mut socket, _) = listener.accept().await.unwrap();
          let count = socket.read(&mut request).await.unwrap();
          let request = String::from_utf8_lossy(&request[..count]);
          let range = request.lines().find_map(|line| {
            line.strip_prefix("range: bytes=").or_else(|| line.strip_prefix("Range: bytes="))
          }).unwrap();
          let (start, end) = range.split_once('-').unwrap();
          let start = start.parse::<usize>().unwrap();
          let end = end.parse::<usize>().unwrap();
          let body = &payload[start..=end];
          let headers = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/16\r\nConnection: close\r\n\r\n",
            body.len()
          );
          socket.write_all(headers.as_bytes()).await.unwrap();
          socket.write_all(body).await.unwrap();
        }
      }
    });
    let cancelled = AtomicBool::new(false);
    let client = Client::builder().no_proxy().build().unwrap();
    let url = format!("http://{address}/fixture.exe");
    let checksum = "0".repeat(64);
    let result = {
      let mut on_progress = |_, _| {};
      let is_cancelled = || cancelled.load(Ordering::SeqCst);
      let download = download_windows_artifact_with_segment_size(
        &client,
        &url,
        16,
        &checksum,
        &partial,
        &target,
        8,
        &mut on_progress,
        &is_cancelled,
      );
      let cancel = async {
        reached.await.unwrap();
        if body_stalled {
          timeout(Duration::from_secs(1), async {
            loop {
              if tokio::fs::metadata(&segments[1].path)
                .await
                .is_ok_and(|metadata| metadata.len() > 0)
              {
                break;
              }
              tokio::time::sleep(Duration::from_millis(10)).await;
            }
          })
          .await
          .expect("the first body byte must reach disk before cancellation");
        }
        cancelled.store(true, Ordering::SeqCst);
      };
      let (result, ()) = tokio::join!(timeout(Duration::from_secs(2), download), cancel);
      result
    };
    let cleaned =
      !partial.exists() && !target.exists() && segments.iter().all(|part| !part.path.exists());
    let error = result
      .expect("cancellation must interrupt a stalled network operation")
      .unwrap_err();
    assert!(error
      .to_string()
      .contains("Windows update download was cancelled"));
    assert!(
      cleaned,
      "explicit cancellation must clean partial and resumable files"
    );
    if body_stalled {
      timeout(
        Duration::from_secs(2),
        download_windows_artifact_with_segment_size(
          &client,
          &url,
          16,
          &hex::encode(Sha256::digest(b"0123456789abcdef")),
          &partial,
          &target,
          8,
          &mut |_, _| {},
          &|| false,
        ),
      )
      .await
      .expect("retry must complete")
      .expect("retry the cancelled download");
      assert_eq!(tokio::fs::read(&target).await.unwrap(), b"0123456789abcdef");
      assert!(!partial.exists());
      assert!(segments.iter().all(|segment| !segment.path.exists()));
    }
    timeout(Duration::from_secs(2), tasks.join_next())
      .await
      .expect("cancelled request must close its connection")
      .unwrap()
      .unwrap();
    std::fs::remove_dir_all(root).unwrap();
  }

  #[tokio::test]
  async fn cancels_while_range_probe_is_stalled() {
    assert_cancels_stalled_download("range").await;
  }

  #[tokio::test]
  async fn cancels_while_segment_headers_are_stalled() {
    assert_cancels_stalled_download("headers").await;
  }

  #[tokio::test]
  async fn cancels_while_segment_body_is_stalled() {
    assert_cancels_stalled_download("body").await;
  }

  #[test]
  fn builds_ordered_segments_without_exceeding_the_artifact() {
    let segments = build_segments(Path::new("fixture.exe.part"), 11, 4).expect("build segments");
    assert_eq!(segments.len(), 3);
    assert_eq!((segments[0].start, segments[0].end), (0, 3));
    assert_eq!((segments[1].start, segments[1].end), (4, 7));
    assert_eq!((segments[2].start, segments[2].end), (8, 10));
  }

  #[tokio::test]
  async fn resumes_completed_segments_and_verifies_the_combined_sha256() {
    let payload = Arc::new(b"parallel-range-download".to_vec());
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind fixture server");
    let address = listener.local_addr().expect("read fixture address");
    let server_payload = Arc::clone(&payload);
    let server_requests = Arc::clone(&requests);
    let server = tokio::spawn(async move {
      let mut connections = Vec::new();
      for _ in 0..4 {
        let (mut socket, _) = listener.accept().await.expect("accept fixture request");
        let payload = Arc::clone(&server_payload);
        let requests = Arc::clone(&server_requests);
        connections.push(tokio::spawn(async move {
          let mut request = vec![0_u8; 2048];
          let count = socket.read(&mut request).await.expect("read fixture request");
          let request = String::from_utf8_lossy(&request[..count]);
          let range = request
            .lines()
            .find_map(|line| line.strip_prefix("range: ").or_else(|| line.strip_prefix("Range: ")))
            .expect("read Range header")
            .to_owned();
          requests.lock().expect("lock requests").push(range.clone());
          let values = range
            .strip_prefix("bytes=")
            .expect("strip range prefix")
            .split_once('-')
            .expect("split range");
          let start = values.0.parse::<usize>().expect("parse range start");
          let end = values.1.parse::<usize>().expect("parse range end");
          let body = &payload[start..=end];
          let response = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nConnection: close\r\n\r\n",
            body.len(),
            payload.len()
          );
          socket
            .write_all(response.as_bytes())
            .await
            .expect("write fixture headers");
          socket.write_all(body).await.expect("write fixture body");
        }));
      }
      for connection in connections {
        connection.await.expect("join fixture connection");
      }
    });

    let root = std::env::temp_dir().join(format!("fabdev-windows-range-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create fixture");
    let partial = root.join("fixture.exe.part");
    let target = root.join("fixture.exe");
    let segments =
      build_segments(&partial, payload.len() as u64, 6).expect("build fixture segments");
    tokio::fs::write(&segments[0].path, &payload[0..6])
      .await
      .expect("write resumed segment");
    let client = Client::builder().build().expect("build fixture client");
    let mut progress = Vec::new();
    download_windows_artifact_with_segment_size(
      &client,
      &format!("http://{address}/fixture.exe"),
      payload.len() as u64,
      &hex::encode(Sha256::digest(payload.as_slice())),
      &partial,
      &target,
      6,
      &mut |downloaded, _| progress.push(downloaded),
      &|| false,
    )
    .await
    .expect("download fixture");
    server.await.expect("join fixture server");

    assert_eq!(
      tokio::fs::read(&target).await.expect("read target"),
      *payload
    );
    assert_eq!(progress.first(), Some(&6));
    assert_eq!(progress.last(), Some(&(payload.len() as u64)));
    let requests = requests.lock().expect("lock requests");
    assert!(requests.contains(&"bytes=0-0".to_owned()));
    assert!(!requests.contains(&"bytes=0-5".to_owned()));
    assert!(segments.iter().all(|segment| !segment.path.exists()));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }

  #[tokio::test]
  async fn explicit_cancellation_removes_resumable_segments() {
    let root = std::env::temp_dir().join(format!("fabdev-windows-cancel-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root)
      .await
      .expect("create fixture");
    let partial = root.join("fixture.exe.part");
    let target = root.join("fixture.exe");
    let segments = build_segments(&partial, 12, 6).expect("build fixture segments");
    tokio::fs::write(&segments[0].path, b"resume")
      .await
      .expect("write resumed segment");
    let client = Client::builder().build().expect("build fixture client");

    let error = download_windows_artifact_with_segment_size(
      &client,
      "http://127.0.0.1:1/fixture.exe",
      12,
      &"0".repeat(64),
      &partial,
      &target,
      6,
      &mut |_, _| {},
      &|| true,
    )
    .await
    .expect_err("cancel download");

    assert!(error.to_string().contains("cancelled"));
    assert!(segments.iter().all(|segment| !segment.path.exists()));
    std::fs::remove_dir_all(root).expect("remove fixture");
  }
}
