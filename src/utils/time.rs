use std::time::{Duration, SystemTime};

pub fn format_time(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

pub fn format_duration(duration_ms: i64) -> String {
    let total_seconds = duration_ms.max(0) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    if hours > 0 {
        format!("{hours} hr {minutes} min")
    } else {
        format!("{minutes} min")
    }
}

pub fn utc_now_millis() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}
