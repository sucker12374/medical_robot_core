use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant};

// 1. 任务队列结构 [cite: 431]
struct TaskQueue {
    tasks: Mutex<VecDeque<String>>,
}

// 2. 区域访问控制 [cite: 432]
struct ZoneControl {
    occupied: Mutex<bool>,
}

// 3. 健康监测系统 [cite: 433]
struct HealthMonitor {
    last_heartbeat: Mutex<Instant>,
}

fn main() {
    // 使用 Arc 包装，以便在多个机器人（线程）之间安全共享状态 [cite: 426, 472]
    let task_queue = Arc::new(TaskQueue {
        tasks: Mutex::new(VecDeque::from([
            "Disinfect Ward A".to_string(),
            "Deliver Medicine to B".to_string(),
        ])),
    });

    let zone = Arc::new(ZoneControl { occupied: Mutex::new(false) });
    let monitor = Arc::new(HealthMonitor { last_heartbeat: Mutex::new(Instant::now()) });

    println!("Medical Care Robot System Starting...");

    // 模拟一个机器人线程 [cite: 439]
    let t_task_queue = Arc::clone(&task_queue);
    let robot_thread = thread::spawn(move || {
        if let Some(task) = t_task_queue.tasks.lock().unwrap().pop_front() {
            println!("Robot 1 started: {}", task);
            // 模拟心跳更新 [cite: 441, 470]
            thread::sleep(Duration::from_secs(1));
        }
    });

    robot_thread.join().unwrap();
    println!("System shutdown gracefully."); [cite: 435]
}