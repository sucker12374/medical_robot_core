use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use project_blaze::deadlock::run_deadlock_prevention_demo;
use project_blaze::dispatch_process::DispatchProcess;
use project_blaze::ffi_metrics::RobotMetrics;
use project_blaze::monitor::{spawn_monitor_thread, HealthMonitor};
use project_blaze::robot::{spawn_robot, RobotBehavior, RobotWorkerConfig};
use project_blaze::task::{CompletedTask, Task, TaskKind};
use project_blaze::task_queue::TaskQueue;
use project_blaze::types::{RobotStatus, SimulationConfig};
use project_blaze::zone::ZoneManager;

const ROBOT_COUNT: u32 = 3;

fn main() {
    // check if re-invoked as the child dispatcher process
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--task-dispatcher") {
            let first_id: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let count: u32   = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            project_blaze::dispatch_process::run_as_child_dispatcher(first_id, count);
            return;
        }
    }
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

    // unnamed pipe and child process
    println!("[dispatch_process] spawning child dispatcher via unnamed pipe...");
    let pipe_tasks_added = match DispatchProcess::spawn(100, 3) {
        Ok(mut dp) => {
            dp.drain_into_queue(&task_queue);
            let _ = dp.wait();
            3usize
        }
        Err(e) => {
            eprintln!("[dispatch_process] could not spawn child: {e} (skipping)");
            0
        }
    };
    let total_tasks = total_static_tasks + pipe_tasks_added;
    println!("[dispatch_process] {pipe_tasks_added} extra task(s) added via pipe");
    println!("[dispatch_process] total tasks in queue: {total_tasks}");
    println!();

    // FFI / raw pointer metrics
    let robot_metrics: Vec<Arc<Mutex<RobotMetrics>>> = (1..=ROBOT_COUNT)
        .map(|id| Arc::new(Mutex::new(RobotMetrics::new(id))))
        .collect();

    let monitor_handle = spawn_monitor_thread(
        Arc::clone(&monitor),
        Arc::clone(&shutdown),
        config.monitor_poll_interval,
        true,
    );

    let robot_handles: Vec<_> = (1..=ROBOT_COUNT)
        .map(|robot_id| {
            let behavior = if robot_id == 3 {
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

    let run_summaries: Vec<_> = robot_handles
        .into_iter()
        .map(|handle| handle.join().expect("robot thread should join cleanly"))
        .collect();
    let offline_events = monitor_handle
        .join()
        .expect("monitor thread should join cleanly");

    // update FFI metrics counters from run summaries
    for summary in &run_summaries {
        let idx = (summary.robot_id - 1) as usize;
        let m = robot_metrics[idx].lock().unwrap();
        for _ in 0..summary.completed_tasks {
            m.record_task_completed();
        }
    }
    
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

    // print FFI metrics
    println!();
    println!("FFI Metrics (raw-pointer C-ABI counter per robot)");
    println!("--------------------------------------------------");
    for m in &robot_metrics {
        println!("  {}", m.lock().unwrap());
    }  // RobotMetrics drop here, metrics_free called, heap memory reclaimed.

    // deadlock prevention demo
    let (successes, contentions) = run_deadlock_prevention_demo();
    println!();
    println!("Deadlock Prevention summary");
    println!("---------------------------");
    println!("  Ordered-lock successes  : {successes}");
    println!("  Contention retries      : {contentions}");
    println!("  Deadlocks possible      : 0  (by construction)");
}
}
