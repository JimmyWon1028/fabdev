use std::future::Future;
use std::time::Duration;

use anyhow::bail;

pub(crate) async fn with_cancellation<T, C>(
  operation: impl Future<Output = anyhow::Result<T>>,
  is_cancelled: &C,
  message: &'static str,
) -> anyhow::Result<T>
where
  C: Fn() -> bool,
{
  let cancelled = async {
    while !is_cancelled() {
      tokio::time::sleep(Duration::from_millis(50)).await;
    }
  };
  tokio::select! {
    biased;
    () = cancelled => bail!(message),
    result = operation => {
      if is_cancelled() {
        bail!(message);
      }
      result
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn cancellation_before_start_does_not_poll_the_operation() {
    let started = std::cell::Cell::new(false);
    let error = with_cancellation(
      async {
        started.set(true);
        Ok(())
      },
      &|| true,
      "cancelled",
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "cancelled");
    assert!(!started.get(), "a cancelled operation must not start");
  }

  #[tokio::test]
  async fn preserves_success_and_failure_when_not_cancelled() {
    assert_eq!(
      with_cancellation(async { Ok(42) }, &|| false, "cancelled")
        .await
        .unwrap(),
      42
    );
    let result: anyhow::Result<()> =
      with_cancellation(async { bail!("original failure") }, &|| false, "cancelled").await;
    assert_eq!(result.unwrap_err().to_string(), "original failure");
  }
}
