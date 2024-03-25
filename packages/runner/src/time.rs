use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct TimePivot {
    instant: Instant,
    time_us: u64,
}

impl TimePivot {
    pub fn build() -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");

        Self {
            instant: Instant::now(),
            time_us: since_the_epoch.as_micros() as u64,
        }
    }

    pub fn started_ms(&self) -> u64 {
        self.time_us / 1000
    }

    pub fn timestamp_ms(&self, now: Instant) -> u64 {
        self.time_us / 1000 + now.duration_since(self.instant).as_millis() as u64
    }

    pub fn started_us(&self) -> u64 {
        self.time_us
    }

    pub fn timestamp_us(&self, now: Instant) -> u64 {
        self.time_us + now.duration_since(self.instant).as_micros() as u64
    }
}

pub struct TimeTicker {
    last_tick: Instant,
    tick_ms: u64,
}

impl TimeTicker {
    pub fn build(tick_ms: u64) -> Self {
        Self { last_tick: Instant::now(), tick_ms }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if now.duration_since(self.last_tick).as_millis() as u64 >= self.tick_ms {
            self.last_tick = now;
            true
        } else {
            false
        }
    }
}
