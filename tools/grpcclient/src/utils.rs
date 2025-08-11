use std::time::{SystemTime, UNIX_EPOCH, Duration};

/// Format Unix timestamp to human-readable date
pub fn format_timestamp(timestamp: u64) -> String {
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    match datetime.duration_since(SystemTime::now()) {
        Ok(future) => format!("in {} seconds", future.as_secs()),
        Err(_) => {
            let past = SystemTime::now().duration_since(datetime).unwrap();
            format!("{} seconds ago", past.as_secs())
        }
    }
}
