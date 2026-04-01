use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::types::{RobotId, RobotStatus};

#[derive(Debug)]
struct MonitorState {
    last_heartbeat: HashMap<RobotId, Instant>,
    statuses: HashMap<RobotId, RobotStatus>,
}

/// OS concept shown here: liveness monitoring in a concurrent system.
///
/// This monitor uses its own mutex, separate from the task queue and zone
/// manager, so health checks do not block normal robot work.
#[derive(Debug)]
pub struct HealthMonitor {
    state: Mutex<MonitorState>,
    timeout: Duration,
}

impl HealthMonitor {
    pub fn new(timeout: Duration) -> Self {
        Self {
            state: Mutex::new(MonitorState {
                last_heartbeat: HashMap::new(),
                statuses: HashMap::new(),
            }),
            timeout,
        }
    }

    pub fn register_robot(&self, robot_id: RobotId) {
        self.register_robot_at(robot_id, Instant::now());
    }

    pub fn register_robot_at(&self, robot_id: RobotId, now: Instant) {
        let mut state = self.state.lock().unwrap();
        state.last_heartbeat.insert(robot_id, now);
        state.statuses.insert(robot_id, RobotStatus::Online);
    }

    pub fn record_heartbeat(&self, robot_id: RobotId) {
        self.record_heartbeat_at(robot_id, Instant::now());
    }

    pub fn record_heartbeat_at(&self, robot_id: RobotId, now: Instant) {
        let mut state = self.state.lock().unwrap();
        state.last_heartbeat.insert(robot_id, now);
        state.statuses.insert(robot_id, RobotStatus::Online);
    }

    pub fn check_timeouts(&self) -> Vec<RobotId> {
        self.check_timeouts_at(Instant::now())
    }

    pub fn check_timeouts_at(&self, now: Instant) -> Vec<RobotId> {
        let mut state = self.state.lock().unwrap();
        // First collect every robot that timed out.
        let candidates: Vec<_> = state
            .last_heartbeat
            .iter()
            .filter_map(|(&robot_id, &last_seen)| {
                (now.duration_since(last_seen) > self.timeout
                    && state.statuses.get(&robot_id) != Some(&RobotStatus::Offline))
                .then_some(robot_id)
            })
            .collect();

        // Then mark them offline. Doing it this way keeps the logic clear.
        for robot_id in &candidates {
            state.statuses.insert(*robot_id, RobotStatus::Offline);
        }

        candidates
    }

    pub fn status(&self, robot_id: RobotId) -> Option<RobotStatus> {
        self.state.lock().unwrap().statuses.get(&robot_id).copied()
    }

    pub fn statuses_snapshot(&self) -> HashMap<RobotId, RobotStatus> {
        self.state.lock().unwrap().statuses.clone()
    }
}

pub fn spawn_monitor_thread(
    monitor: Arc<HealthMonitor>,
    shutdown: Arc<Mutex<bool>>,
    poll_interval: Duration,
    verbose: bool,
) -> JoinHandle<Vec<RobotId>> {
    thread::spawn(move || {
        let mut offline_events = Vec::new();

        loop {
            if should_shutdown(&shutdown) {
                offline_events.extend(monitor.check_timeouts());
                break;
            }

            // OS concept: a monitor thread runs at the same time as worker
            // threads and checks for timeout events.
            for robot_id in monitor.check_timeouts() {
                if verbose {
                    println!(
                        "[monitor] robot {robot_id} marked OFFLINE after heartbeat timeout"
                    );
                }
                offline_events.push(robot_id);
            }

            thread::sleep(poll_interval);
        }

        offline_events
    })
}

fn should_shutdown(shutdown: &Arc<Mutex<bool>>) -> bool {
    *shutdown.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::HealthMonitor;
    use crate::types::RobotStatus;

    #[test]
    fn registered_robot_starts_online() {
        let monitor = HealthMonitor::new(Duration::from_secs(1));

        monitor.register_robot(1);

        assert_eq!(monitor.status(1), Some(RobotStatus::Online));
    }

    #[test]
    fn regular_heartbeat_keeps_robot_online() {
        let timeout = Duration::from_millis(100);
        let monitor = HealthMonitor::new(timeout);
        let start = Instant::now();

        monitor.register_robot_at(1, start);
        monitor.record_heartbeat_at(1, start + Duration::from_millis(40));
        let newly_offline = monitor.check_timeouts_at(start + Duration::from_millis(90));

        assert!(newly_offline.is_empty());
        assert_eq!(monitor.status(1), Some(RobotStatus::Online));
    }

    #[test]
    fn missing_heartbeat_marks_robot_offline() {
        let timeout = Duration::from_millis(50);
        let monitor = HealthMonitor::new(timeout);
        let start = Instant::now();

        monitor.register_robot_at(7, start);
        let newly_offline = monitor.check_timeouts_at(start + Duration::from_millis(80));

        assert_eq!(newly_offline, vec![7]);
        assert_eq!(monitor.status(7), Some(RobotStatus::Offline));
    }

    #[test]
    fn timeout_detection_is_repeatable() {
        let timeout = Duration::from_millis(50);
        let monitor = HealthMonitor::new(timeout);
        let start = Instant::now();

        monitor.register_robot_at(9, start);

        let first = monitor.check_timeouts_at(start + Duration::from_millis(80));
        let second = monitor.check_timeouts_at(start + Duration::from_millis(120));

        assert_eq!(first, vec![9]);
        assert!(second.is_empty());
        assert_eq!(monitor.status(9), Some(RobotStatus::Offline));
    }
}
