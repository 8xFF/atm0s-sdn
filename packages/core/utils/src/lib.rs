pub trait Timer: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Clone)]
pub struct SystemTimer();

impl Timer for SystemTimer {
    fn now_ms(&self) -> u64 {
        //TODO
        0
    }
}