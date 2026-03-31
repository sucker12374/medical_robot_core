use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use project_blaze::monitor::{spawn_monitor_thread, HealthMonitor};
use project_blaze::robot::{spawn_robot, RobotBehavior, RobotWorkerConfig};
use project_blaze::task::{CompletedTask, Task, TaskKind};
use project_blaze::task_queue::TaskQueue;
use project_blaze::types::RobotStatus;
use project_blaze::zone::ZoneManager;

#[test]
fn simulation_completes_all_tasks_without_duplication_or_zone_leaks() {
    let tasks = vec![
        Task::new(1, "Zone-A", TaskKind::Delivery),
        Task::new(2, "Zone-A", TaskKind::Disinfection),
        Task::new(3, "Zone-B", TaskKind::SurgicalAssist),
        Task::new(4, "Zone-B", TaskKind::Delivery),
        Task::new(5, "Zone-C", TaskKind::Disinfection),
        Task::new(6, "Zone-A", TaskKind::Delivery),
    ];
    let total_tasks = tasks.len();

    let task_queue = Arc::new(TaskQueue::from_tasks(tasks));
    let zone_manager = Arc::new(ZoneManager::new([
        "Zone-A".to_string(),
        "Zone-B".to_string(),
        "Zone-C".to_string(),
    ]));
    let monitor = Arc::new(HealthMonitor::new(Duration::from_secs(5)));
    let completed_tasks: Arc<Mutex<Vec<CompletedTask>>> = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(Mutex::new(false));

    let monitor_handle = spawn_monitor_thread(
        Arc::clone(&monitor),
        Arc::clone(&shutdown),
        Duration::from_millis(40),
        false,
    );

    let handles: Vec<_> = (1..=3)
        .map(|robot_id| {
            spawn_robot(
                RobotWorkerConfig {
                    robot_id,
                    heartbeat_interval: Duration::from_millis(50),
                    zone_retry_interval: Duration::from_millis(20),
                    idle_sleep_interval: Duration::from_millis(10),
                    task_execution_duration: Duration::from_millis(35),
                    behavior: RobotBehavior::Normal,
                    verbose: false,
                },
                Arc::clone(&task_queue),
                Arc::clone(&zone_manager),
                Arc::clone(&monitor),
                Arc::clone(&completed_tasks),
                Arc::clone(&shutdown),
            )
        })
        .collect();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if completed_tasks.lock().unwrap().len() == total_tasks {
            break;
        }

        assert!(Instant::now() < deadline, "simulation did not complete in time");
        thread::sleep(Duration::from_millis(10));
    }

    *shutdown.lock().unwrap() = true;

    for handle in handles {
        let summary = handle.join().unwrap();
        assert!(!summary.simulated_failure);
    }

    let offline_events = monitor_handle.join().unwrap();
    assert!(offline_events.is_empty());

    let completed = completed_tasks.lock().unwrap();
    assert_eq!(completed.len(), total_tasks);

    let unique_task_ids = completed
        .iter()
        .map(|entry| entry.task_id)
        .collect::<HashSet<_>>();
    assert_eq!(unique_task_ids.len(), total_tasks);
    assert!(task_queue.is_empty());
    assert!(zone_manager.snapshot().values().all(Option::is_none));

    for robot_id in 1..=3 {
        assert_eq!(monitor.status(robot_id), Some(RobotStatus::Online));
    }
}
