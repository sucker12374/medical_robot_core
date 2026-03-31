use std::time::Duration;

pub type RobotId = u32;
pub type TaskId = u32;
pub type ZoneId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub heartbeat_interval: Duration,
    pub monitor_poll_interval: Duration,
    pub heartbeat_timeout: Duration,
    pub task_execution_duration: Duration,
    pub zone_retry_duration: Duration,
    pub idle_sleep_duration: Duration,
}
