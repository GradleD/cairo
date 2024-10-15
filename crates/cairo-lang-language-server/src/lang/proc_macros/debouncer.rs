use std::time::{Duration, SystemTime};

pub struct Debouncer {
    time: Duration,
    last_run: SystemTime,
}

impl Debouncer {
    pub fn new(time: Duration) -> Self {
        Self {
            time,
            // We want it to run on first call to [`Self::run_debounced`].
            last_run: SystemTime::now() - time,
        }
    }

    pub fn run_debounced(&mut self, mut job: impl FnMut()) {
        if self.last_run.elapsed().unwrap() >= self.time {
            self.last_run = SystemTime::now();

            job()
        }
    }
}
