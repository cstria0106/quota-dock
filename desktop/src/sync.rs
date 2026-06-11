use std::time::{Duration, Instant};

use crate::settings::MIN_SYNC_INTERVAL_SECS;

#[derive(Debug, Default)]
pub struct SyncScheduler {
    last_started_at: Option<Instant>,
    manual_requested: bool,
}

impl SyncScheduler {
    pub fn request_now(&mut self) {
        self.manual_requested = true;
    }

    pub fn should_start(&self, now: Instant, interval_secs: u64, is_running: bool) -> bool {
        if is_running {
            return false;
        }
        if self.manual_requested {
            return true;
        }
        let Some(last_started_at) = self.last_started_at else {
            return true;
        };
        now.duration_since(last_started_at) >= sync_interval(interval_secs)
    }

    pub fn mark_started(&mut self, now: Instant) {
        self.last_started_at = Some(now);
        self.manual_requested = false;
    }
}

pub fn sync_interval(interval_secs: u64) -> Duration {
    Duration::from_secs(interval_secs.max(MIN_SYNC_INTERVAL_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_triggers_again_without_data_change() {
        let mut scheduler = SyncScheduler::default();
        let started = Instant::now();

        assert!(scheduler.should_start(started, 60, false));
        scheduler.mark_started(started);

        assert!(!scheduler.should_start(started + Duration::from_secs(59), 60, false));
        assert!(scheduler.should_start(started + Duration::from_secs(60), 60, false));
    }

    #[test]
    fn manual_request_waits_for_running_sync_to_finish() {
        let mut scheduler = SyncScheduler::default();
        scheduler.request_now();

        assert!(!scheduler.should_start(Instant::now(), 60, true));
        assert!(scheduler.should_start(Instant::now(), 60, false));
    }
}
