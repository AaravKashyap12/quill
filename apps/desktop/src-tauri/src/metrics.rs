use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricEvent<'a> {
    pub timestamp_unix_ms: u128,
    pub metric: &'a str,
    pub value_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountMetricEvent<'a> {
    pub timestamp_unix_ms: u128,
    pub metric: &'a str,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<&'a str>,
}

pub fn record(
    metric: &str,
    value_ms: u128,
    mode: Option<&str>,
    detail: Option<&str>,
) -> Result<()> {
    let event = MetricEvent {
        timestamp_unix_ms: timestamp_unix_ms(),
        metric,
        value_ms,
        mode,
        detail,
    };
    append(&event)
}

pub fn increment(metric: &str, mode: Option<&str>, detail: Option<&str>) -> Result<()> {
    let event = CountMetricEvent {
        timestamp_unix_ms: timestamp_unix_ms(),
        metric,
        count: 1,
        mode,
        detail,
    };
    append(&event)
}

fn timestamp_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn append(event: &impl Serialize) -> Result<()> {
    let path = metrics_path()?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open metrics log: {}", path.display()))?;
    serde_json::to_writer(&mut file, &event)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn metrics_path() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("Windows did not provide a local application-data directory")?
        .join("quill")
        .join("logs")
        .join("metrics.ndjson"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_metrics_do_not_masquerade_as_durations() {
        let event = CountMetricEvent {
            timestamp_unix_ms: 123,
            metric: "scribeCleanupSafetyFallback",
            count: 1,
            mode: Some("scribe"),
            detail: Some("addedCommitment"),
        };
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["count"], 1);
        assert!(value.get("valueMs").is_none());
    }
}
