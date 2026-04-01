use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use project_blaze::monitor::{spawn_monitor_thread, HealthMonitor};
use project_blaze::robot::{spawn_robot, RobotBehavior, RobotWorkerConfig};
use project_blaze::task::{CompletedTask, Task, TaskKind};
use project_blaze::task_queue::TaskQueue;
use project_blaze::types::{RobotStatus, SimulationConfig};
use project_blaze::zone::ZoneManager;

const ROBOT_COUNT: u32 = 3;

fn main() {
    // These values control the demo timing and make the output easy to follow.
    let config = SimulationConfig {
        heartbeat_interval: Duration::from_millis(200),
        monitor_poll_interval: Duration::from_millis(150),
        heartbeat_timeout: Duration::from_millis(700),
        task_execution_duration: Duration::from_millis(350),
        zone_retry_duration: Duration::from_millis(120),
        idle_sleep_duration: Duration::from_millis(75),
    };

    let tasks = vec![
        Task::new(1, "Zone-A", TaskKind::Delivery),
        Task::new(2, "Zone-A", TaskKind::Disinfection),
        Task::new(3, "Zone-B", TaskKind::SurgicalAssist),
        Task::new(4, "Zone-A", TaskKind::Delivery),
        Task::new(5, "Zone-C", TaskKind::Disinfection),
        Task::new(6, "Zone-B", TaskKind::Delivery),
    ];
    let total_tasks = tasks.len();

    // Shared state is split by concern:
    // task queue lock for tasks,
    // zone manager lock for room access,
    // monitor lock for robot health.
    let task_queue = Arc::new(TaskQueue::from_tasks(tasks));
    let zone_manager = Arc::new(ZoneManager::new([
        "Zone-A".to_string(),
        "Zone-B".to_string(),
        "Zone-C".to_string(),
    ]));
    let monitor = Arc::new(HealthMonitor::new(config.heartbeat_timeout));
    let completed_tasks: Arc<Mutex<Vec<CompletedTask>>> = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(Mutex::new(false));

    println!("Project Blaze: Medical Care Robot Coordination System");
    println!("----------------------------------------------------");
    println!("Robots: {ROBOT_COUNT}, tasks: {total_tasks}");
    println!("Contention demo: multiple tasks target Zone-A");
    println!("Timeout demo: robot 3 stops heartbeating after a few heartbeats");
    println!();

    let monitor_handle = spawn_monitor_thread(
        Arc::clone(&monitor),
        Arc::clone(&shutdown),
        config.monitor_poll_interval,
        true,
    );

    // Spawn several robots so the demo shows real contention.
    let robot_handles: Vec<_> = (1..=ROBOT_COUNT)
        .map(|robot_id| {
            let behavior = if robot_id == 3 {
                // One robot fails on purpose so the health monitor can mark it offline.
                RobotBehavior::FailAfterHeartbeats(3)
            } else {
                RobotBehavior::Normal
            };

            spawn_robot(
                RobotWorkerConfig {
                    robot_id,
                    heartbeat_interval: config.heartbeat_interval,
                    zone_retry_interval: config.zone_retry_duration,
                    idle_sleep_interval: config.idle_sleep_duration,
                    task_execution_duration: config.task_execution_duration,
                    behavior,
                    verbose: true,
                },
                Arc::clone(&task_queue),
                Arc::clone(&zone_manager),
                Arc::clone(&monitor),
                Arc::clone(&completed_tasks),
                Arc::clone(&shutdown),
            )
        })
        .collect();

    // Wait until both key events happen:
    // all tasks are done and one robot is marked offline.
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let completed = completed_tasks.lock().unwrap().len();
        let robot_three_offline = monitor.status(3) == Some(RobotStatus::Offline);

        if completed == total_tasks && robot_three_offline {
            println!();
            println!("[main] all tasks completed and offline detection observed");
            break;
        }

        if Instant::now() >= deadline {
            println!();
            println!("[main] demo deadline reached, beginning controlled shutdown");
            break;
        }

        thread::sleep(Duration::from_millis(100));
    }

    *shutdown.lock().unwrap() = true;

    // Join all threads for a clean shutdown.
    let run_summaries: Vec<_> = robot_handles
        .into_iter()
        .map(|handle| handle.join().expect("robot thread should join cleanly"))
        .collect();
    let offline_events = monitor_handle
        .join()
        .expect("monitor thread should join cleanly");

    println!();
    println!("Run summary");
    println!("-----------");
    for summary in run_summaries {
        println!(
            "robot {} completed {} task(s){}",
            summary.robot_id,
            summary.completed_tasks,
            if summary.simulated_failure {
                " before simulated failure"
            } else {
                ""
            }
        );
    }

    let completed = completed_tasks.lock().unwrap();
    println!("completed task ids: {:?}", completed.iter().map(|task| task.task_id).collect::<Vec<_>>());
    println!("offline events: {:?}", offline_events);
    println!("final zone occupancy: {:?}", zone_manager.snapshot());
}
