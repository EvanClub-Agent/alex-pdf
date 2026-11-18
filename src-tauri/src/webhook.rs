//! 任务终态后向 webhook URL POST JSON 通知。
//! 10 秒超时；失败按 1s / 3s / 9s 指数退避，共最多 3 次尝试。

use std::time::Duration;

use tracing::{info, warn};

use crate::state::TaskStatusResponse;

const ATTEMPTS: usize = 3;
const TIMEOUT_SECS: u64 = 10;

pub async fn deliver(url: &str, payload: &TaskStatusResponse) {
  let client = match reqwest::Client::builder()
    .timeout(Duration::from_secs(TIMEOUT_SECS))
    .build()
  {
    Ok(c) => c,
    Err(err) => {
      warn!("webhook client build failed: {}", err);
      return;
    }
  };

  let body = match serde_json::to_value(payload) {
    Ok(v) => v,
    Err(err) => {
      warn!("webhook serialize failed: {}", err);
      return;
    }
  };

  for attempt in 0..ATTEMPTS {
    match client.post(url).json(&body).send().await {
      Ok(resp) if resp.status().is_success() => {
        info!("webhook delivered: {} -> {}", payload.task_id, url);
        return;
      }
      Ok(resp) => {
        warn!(
          "webhook attempt {} got {} for {}",
          attempt + 1,
          resp.status(),
          url
        );
      }
      Err(err) => {
        warn!("webhook attempt {} failed: {}", attempt + 1, err);
      }
    }
    if attempt + 1 < ATTEMPTS {
      // 指数退避：1s -> 3s -> 9s
      let backoff = 3u64.pow(attempt as u32);
      tokio::time::sleep(Duration::from_secs(backoff)).await;
    }
  }
  warn!("webhook gave up after {} attempts: {}", ATTEMPTS, url);
}
