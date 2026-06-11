use std::time::{Duration, Instant};

use crate::settings::MIN_SYNC_INTERVAL_SECS;

pub const BOARD_SYNC_INTERVAL: Duration = Duration::from_secs(5);
pub const BOARD_RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncDecision {
    pub refresh_usage: bool,
}

#[derive(Debug, Default)]
pub struct SyncScheduler {
    last_usage_started_at: Option<Instant>,
    last_board_started_at: Option<Instant>,
    refresh_requested: bool,
    send_requested: bool,
}

impl SyncScheduler {
    pub fn request_now(&mut self) {
        self.refresh_requested = true;
        self.send_requested = true;
    }

    pub fn request_send_now(&mut self) {
        self.send_requested = true;
    }

    pub fn should_start(
        &self,
        now: Instant,
        interval_secs: u64,
        last_sync_ok: Option<bool>,
        has_cached_snapshot: bool,
        is_running: bool,
    ) -> Option<SyncDecision> {
        if is_running {
            return None;
        }

        if self.refresh_requested {
            return Some(SyncDecision {
                refresh_usage: true,
            });
        }
        if self.send_requested {
            return Some(SyncDecision {
                refresh_usage: !has_cached_snapshot,
            });
        }

        let usage_due = self.last_usage_started_at.is_none_or(|last_started_at| {
            now.duration_since(last_started_at) >= sync_interval(interval_secs)
        });
        let board_due = self.last_board_started_at.is_none_or(|last_started_at| {
            now.duration_since(last_started_at) >= board_interval(last_sync_ok)
        });
        if !has_cached_snapshot || board_due {
            return Some(SyncDecision {
                refresh_usage: !has_cached_snapshot || usage_due,
            });
        }

        None
    }

    pub fn mark_started(&mut self, now: Instant, refresh_usage: bool) {
        self.last_board_started_at = Some(now);
        if refresh_usage {
            self.last_usage_started_at = Some(now);
        }
        self.refresh_requested = false;
        self.send_requested = false;
    }
}

pub fn sync_interval(interval_secs: u64) -> Duration {
    Duration::from_secs(interval_secs.max(MIN_SYNC_INTERVAL_SECS))
}

pub fn board_interval(last_sync_ok: Option<bool>) -> Duration {
    if matches!(last_sync_ok, Some(false)) {
        BOARD_RETRY_INTERVAL
    } else {
        BOARD_SYNC_INTERVAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_interval_triggers_without_usage_refresh() {
        let mut scheduler = SyncScheduler::default();
        let started = Instant::now();

        assert_eq!(
            scheduler.should_start(started, 60, None, false, false),
            Some(SyncDecision {
                refresh_usage: true
            })
        );
        scheduler.mark_started(started, true);

        assert_eq!(
            scheduler.should_start(
                started + Duration::from_secs(4),
                60,
                Some(true),
                true,
                false
            ),
            None
        );
        assert_eq!(
            scheduler.should_start(
                started + Duration::from_secs(5),
                60,
                Some(true),
                true,
                false
            ),
            Some(SyncDecision {
                refresh_usage: false
            })
        );
    }

    #[test]
    fn usage_refresh_waits_for_board_interval() {
        let mut scheduler = SyncScheduler::default();
        let started = Instant::now();
        scheduler.mark_started(started, true);
        scheduler.mark_started(started + Duration::from_secs(55), false);

        assert_eq!(
            scheduler.should_start(
                started + Duration::from_secs(59),
                60,
                Some(true),
                true,
                false
            ),
            None
        );
        assert_eq!(
            scheduler.should_start(
                started + Duration::from_secs(60),
                60,
                Some(true),
                true,
                false
            ),
            Some(SyncDecision {
                refresh_usage: true
            })
        );
    }

    #[test]
    fn failed_board_sync_retries_after_one_second() {
        let mut scheduler = SyncScheduler::default();
        let started = Instant::now();
        scheduler.mark_started(started, true);

        assert_eq!(
            scheduler.should_start(
                started + Duration::from_millis(999),
                60,
                Some(false),
                true,
                false
            ),
            None
        );
        assert_eq!(
            scheduler.should_start(
                started + Duration::from_secs(1),
                60,
                Some(false),
                true,
                false
            ),
            Some(SyncDecision {
                refresh_usage: false
            })
        );
    }

    #[test]
    fn manual_request_waits_for_running_sync_to_finish() {
        let mut scheduler = SyncScheduler::default();
        scheduler.request_now();

        assert_eq!(
            scheduler.should_start(Instant::now(), 60, None, false, true),
            None
        );
        assert_eq!(
            scheduler.should_start(Instant::now(), 60, None, false, false),
            Some(SyncDecision {
                refresh_usage: true
            })
        );
    }

    #[test]
    fn send_request_reuses_cached_usage_snapshot() {
        let mut scheduler = SyncScheduler::default();
        scheduler.request_send_now();

        assert_eq!(
            scheduler.should_start(Instant::now(), 60, Some(true), true, false),
            Some(SyncDecision {
                refresh_usage: false
            })
        );
    }
}
