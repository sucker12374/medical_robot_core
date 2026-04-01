use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::monitor::HealthMonitor;
use crate::task::CompletedTask;
use crate::task_queue::TaskQueue;
use crate::types::RobotId;
use crate::zone::ZoneManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotBehavior {
    Normal,
    FailAfterHeartbeats(usize),
}

#[derive(Debug, Clone)]
pub struct RobotWorkerConfig {
    pub robot_id: RobotId,
    pub heartbeat_interval: Duration,
    pub zone_retry_interval: Duration,
    pub idle_sleep_interval: Duration,
    pub task_execution_duration: Duration,
    pub behavior: RobotBehavior,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobotRunSummary {
    pub robot_id: RobotId,
    pub completed_tasks: usize,
    pub simulated_failure: bool,
}

pub fn spawn_robot(
    config: RobotWorkerConfig,
    task_queue: Arc<TaskQueue>,
    zone_manager: Arc<ZoneManager>,
    monitor: Arc<HealthMonitor>,
    completed_tasks: Arc<Mutex<Vec<CompletedTask>>>,
    shutdown: Arc<Mutex<bool>>,
) -> JoinHandle<RobotRunSummary> {
    thread::spawn(move || {
        // Each robot starts by telling the monitor that it is alive.
        monitor.register_robot(config.robot_id);
        let mut completed_count = 0usize;
        let mut heartbeat_count = 0usize;
        let mut simulated_failure = false;
        let mut last_heartbeat_at = Instant::now()
            .checked_sub(config.heartbeat_interval)
            .unwrap_or_else(Instant::now);

        loop {
            if should_shutdown(&shutdown) {
                log(config.verbose, config.robot_id, "shutdown signal received");
                break;
            }

            // Robot 3 uses this path in the demo to show timeout detection.
            if should_fail(config.behavior, heartbeat_count) {
                log(
                    config.verbose,
                    config.robot_id,
                    "simulating failure: heartbeats stopped and robot leaves service",
                );
                simulated_failure = true;
                break;
            }

            maybe_send_heartbeat(&config, &monitor, &mut last_heartbeat_at, &mut heartbeat_count);

            // OS concept: many worker threads compete for tasks from one shared queue.
            let Some(task) = task_queue.fetch_task() else {
                thread::sleep(config.idle_sleep_interval);
                continue;
            };

            log(
                config.verbose,
                config.robot_id,
                &format!(
                    "fetched task {} ({:?}) for {}",
                    task.id, task.kind, task.target_zone
                ),
            );

            loop {
                if should_shutdown(&shutdown) {
                    log(
                        config.verbose,
                        config.robot_id,
                        &format!("shutdown requested before task {} could start", task.id),
                    );
                    return RobotRunSummary {
                        robot_id: config.robot_id,
                        completed_tasks: completed_count,
                        simulated_failure,
                    };
                }

                maybe_send_heartbeat(
                    &config,
                    &monitor,
                    &mut last_heartbeat_at,
                    &mut heartbeat_count,
                );

                log(
                    config.verbose,
                    config.robot_id,
                    &format!("requesting access to {}", task.target_zone),
                );

                // OS concept: only one robot may enter a zone at a time.
                if zone_manager.try_acquire(&task.target_zone, config.robot_id) {
                    log(
                        config.verbose,
                        config.robot_id,
                        &format!("acquired {} for task {}", task.target_zone, task.id),
                    );
                    break;
                }

                log(
                    config.verbose,
                    config.robot_id,
                    &format!("{} is busy, retrying soon", task.target_zone),
                );
                thread::sleep(config.zone_retry_interval);
            }

            // Important design choice:
            // the robot does not hold any mutex while it is doing the task.
            // This keeps lock time short and avoids blocking other threads.
            thread::sleep(config.task_execution_duration);

            zone_manager
                .release(&task.target_zone, config.robot_id)
                .expect("robot should release only the zone it owns");

            // Save a simple record so tests can prove every task finished once.
            completed_tasks
                .lock()
                .unwrap()
                .push(CompletedTask::from_task(&task, config.robot_id));

            completed_count += 1;

            log(
                config.verbose,
                config.robot_id,
                &format!("completed task {} and released {}", task.id, task.target_zone),
            );
        }

        RobotRunSummary {
            robot_id: config.robot_id,
            completed_tasks: completed_count,
            simulated_failure,
        }
    })
}

fn maybe_send_heartbeat(
    config: &RobotWorkerConfig,
    monitor: &HealthMonitor,
    last_heartbeat_at: &mut Instant,
    heartbeat_count: &mut usize,
) {
    if last_heartbeat_at.elapsed() >= config.heartbeat_interval {
        // OS concept: periodic heartbeat for liveness detection.
        monitor.record_heartbeat(config.robot_id);
        *last_heartbeat_at = Instant::now();
        *heartbeat_count += 1;
        log(config.verbose, config.robot_id, "heartbeat sent");
    }
}

fn should_fail(behavior: RobotBehavior, heartbeat_count: usize) -> bool {
    matches!(
        behavior,
        RobotBehavior::FailAfterHeartbeats(limit) if heartbeat_count >= limit
    )
}

fn should_shutdown(shutdown: &Arc<Mutex<bool>>) -> bool {
    *shutdown.lock().unwrap()
}

fn log(verbose: bool, robot_id: RobotId, message: &str) {
    if verbose {
        println!("[robot {robot_id}] {message}");
    }
}
