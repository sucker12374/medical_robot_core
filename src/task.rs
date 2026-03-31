use crate::types::{RobotId, TaskId, ZoneId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskKind {
    Delivery,
    Disinfection,
    SurgicalAssist,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Task {
    pub id: TaskId,
    pub target_zone: ZoneId,
    pub kind: TaskKind,
}

impl Task {
    pub fn new(id: TaskId, target_zone: impl Into<ZoneId>, kind: TaskKind) -> Self {
        Self {
            id,
            target_zone: target_zone.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedTask {
    pub task_id: TaskId,
    pub robot_id: RobotId,
    pub zone_id: ZoneId,
    pub kind: TaskKind,
}

impl CompletedTask {
    pub fn from_task(task: &Task, robot_id: RobotId) -> Self {
        Self {
            task_id: task.id,
            robot_id,
            zone_id: task.target_zone.clone(),
            kind: task.kind.clone(),
        }
    }
}
