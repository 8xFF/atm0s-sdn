use std::time::{SystemTime, UNIX_EPOCH};

pub trait Timer: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Clone)]
pub struct SystemTimer();

impl Timer for SystemTimer {
    fn now_ms(&self) -> u64 {
        let start = SystemTime::now();
        start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }
}
