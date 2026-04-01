use std::collections::VecDeque;
use std::sync::Mutex;

use crate::task::Task;

/// OS concept shown here: shared task scheduling with safe synchronization.
///
/// This queue is protected by a mutex so many robot threads can ask for work
/// without taking the same task twice.
#[derive(Debug)]
pub struct TaskQueue {
    queue: Mutex<VecDeque<Task>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn from_tasks(tasks: impl IntoIterator<Item = Task>) -> Self {
        let queue = tasks.into_iter().collect::<VecDeque<_>>();
        Self {
            queue: Mutex::new(queue),
        }
    }

    pub fn add_task(&self, task: Task) {
        // Only hold the lock long enough to change the queue.
        self.queue.lock().unwrap().push_back(task);
    }

    pub fn fetch_task(&self) -> Option<Task> {
        // OS concept: mutual exclusion on shared data.
        // pop_front happens while the mutex is held, so one task can only be
        // removed by one robot thread.
        self.queue.lock().unwrap().pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;

    use super::TaskQueue;
    use crate::task::{Task, TaskKind};

    fn sample_task(id: u32) -> Task {
        Task::new(id, format!("Zone-{id}"), TaskKind::Delivery)
    }

    #[test]
    fn enqueue_and_dequeue_single_thread() {
        let queue = TaskQueue::new();

        queue.add_task(sample_task(1));
        queue.add_task(sample_task(2));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.fetch_task().unwrap().id, 1);
        assert_eq!(queue.fetch_task().unwrap().id, 2);
        assert!(queue.fetch_task().is_none());
    }

    #[test]
    fn concurrent_fetching_consumes_all_tasks() {
        let task_count = 20;
        let queue = Arc::new(TaskQueue::from_tasks((1..=task_count).map(sample_task)));
        let results = Arc::new(Mutex::new(Vec::new()));
        let barrier = Arc::new(Barrier::new(5));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let queue = Arc::clone(&queue);
                let results = Arc::clone(&results);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    while let Some(task) = queue.fetch_task() {
                        results.lock().unwrap().push(task.id);
                    }
                })
            })
            .collect();

        barrier.wait();

        for handle in handles {
            handle.join().unwrap();
        }

        let fetched = results.lock().unwrap();
        assert_eq!(fetched.len(), task_count as usize);
        assert!(queue.is_empty());
    }

    #[test]
    fn concurrent_fetching_assigns_no_duplicate_tasks() {
        let task_count = 32;
        let queue = Arc::new(TaskQueue::from_tasks((1..=task_count).map(sample_task)));
        let fetched_ids = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let queue = Arc::clone(&queue);
                let fetched_ids = Arc::clone(&fetched_ids);
                thread::spawn(move || {
                    while let Some(task) = queue.fetch_task() {
                        fetched_ids.lock().unwrap().push(task.id);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let fetched = fetched_ids.lock().unwrap();
        let unique = fetched.iter().copied().collect::<HashSet<_>>();

        assert_eq!(fetched.len(), task_count as usize);
        assert_eq!(unique.len(), task_count as usize);
    }

    #[test]
    fn queue_is_empty_after_all_tasks_are_consumed() {
        let queue = TaskQueue::from_tasks((1..=4).map(sample_task));

        while queue.fetch_task().is_some() {}

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }
}
