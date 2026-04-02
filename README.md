# Project Blaze: Medical Care Robot Coordination System

A lightweight OS core for coordinating autonomous medical robots, demonstrating core concurrency concepts: thread-safe task distribution, mutual exclusion for shared zones, and heartbeat-based failure detection — all implemented in Rust.

---

## Overview

Modern hospitals deploy autonomous robots for delivery, disinfection, and surgical assistance. This project simulates the coordination layer required to manage multiple robots safely in shared environments. Each robot runs as an independent thread, competing for tasks and hospital zones, while a health monitor tracks liveness via heartbeats.

The system is intentionally minimal — no preemption, no complex scheduling — just correct, safe concurrency using Rust's synchronization primitives.

---

## Features

- **Thread-safe task queue** — FIFO queue backed by `Mutex<VecDeque<Task>>`; concurrent robots fetch tasks without duplication or loss
- **Zone access control** — Mutex-protected occupancy map ensures no two robots occupy the same zone simultaneously
- **Health monitor** — Tracks per-robot heartbeat timestamps and marks robots offline after a configurable timeout
- **Simulated failure** — Robot 3 is configured to stop heartbeating after a fixed number of beats, triggering offline detection
- **Graceful shutdown** — All threads respond to a shared `Arc<Mutex<bool>>` shutdown signal

---

## Project Structure

```
project_blaze/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point, wires all components together
    ├── types.rs         # Shared type aliases (RobotId, TaskId, ZoneId, RobotStatus, SimulationConfig)
    ├── task.rs          # Task and TaskKind definitions, CompletedTask
    ├── task_queue.rs    # Thread-safe FIFO TaskQueue
    ├── zone.rs          # ZoneManager — mutual exclusion for hospital zones
    ├── robot.rs         # Robot worker threads and RobotBehavior
    ├── monitor.rs       # HealthMonitor and background monitor thread
    └── simulation.rs    # (Additional simulation utilities)
```

---

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (edition 2021 or later)

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

### Test

```bash
cargo test
```

---

## Demo Scenario

The default demo (`main.rs`) runs **3 robots** against **6 tasks** across three zones (`Zone-A`, `Zone-B`, `Zone-C`):

| Task | Zone   | Kind           |
|------|--------|----------------|
| 1    | Zone-A | Delivery       |
| 2    | Zone-A | Disinfection   |
| 3    | Zone-B | SurgicalAssist |
| 4    | Zone-A | Delivery       |
| 5    | Zone-C | Disinfection   |
| 6    | Zone-B | Delivery       |

Multiple tasks target `Zone-A` to demonstrate contention handling. Robot 3 is configured with `FailAfterHeartbeats(3)`, simulating a hardware failure — the monitor detects the missed heartbeats and marks it offline.

**Expected output includes:**
- Robots concurrently fetching and executing tasks
- Zone contention retries logged in real time
- Robot 3 being marked `OFFLINE` by the monitor thread
- A run summary with per-robot task counts and offline events

---

## Configuration

Timing parameters are set in `main.rs` via `SimulationConfig`:

| Parameter                | Default  | Description                                    |
|--------------------------|----------|------------------------------------------------|
| `heartbeat_interval`     | 200 ms   | How often robots send a heartbeat              |
| `monitor_poll_interval`  | 150 ms   | How often the monitor checks for timeouts      |
| `heartbeat_timeout`      | 700 ms   | Time without a heartbeat before marking offline|
| `task_execution_duration`| 350 ms   | Simulated task execution time                  |
| `zone_retry_duration`    | 120 ms   | Sleep between zone acquisition retries         |
| `idle_sleep_duration`    | 75 ms    | Sleep when no tasks are available              |

---

## Architecture

```
Main Thread
├── TaskQueue       — holds and manages pending tasks
├── ZoneManager     — enforces mutual exclusion per zone
├── HealthMonitor   — tracks heartbeats, detects failures
│
├── Robot Thread 1  ─┐
├── Robot Thread 2   ├─ fetch tasks → acquire zone → execute → release
├── Robot Thread 3  ─┘
│
└── Monitor Thread  — polls HealthMonitor, logs offline events
```

All shared components are wrapped in `Arc` for shared ownership across threads. Internal state is protected by `Mutex`.

---

## Synchronization Design

| Component      | Primitive                        | Rationale                                              |
|----------------|----------------------------------|--------------------------------------------------------|
| Task queue     | `Mutex<VecDeque<Task>>`          | Simple, correct serialization of concurrent pops       |
| Zone occupancy | `Mutex<HashMap<ZoneId, Option<RobotId>>>` | Atomic check-and-set for zone acquisition     |
| Health monitor | `Mutex<MonitorState>`            | Single lock over both heartbeat and status maps        |
| Shutdown flag  | `Arc<Mutex<bool>>`               | Consistent with other shared state; overhead negligible|

`RwLock` was evaluated but rejected — write starvation was observed under stress testing, and the read-heavy assumption does not hold for this workload. `SpinLock` was also considered but ruled out due to unacceptable CPU usage on low-power hardware.

---

## Test Coverage

Tests are colocated with their modules using `#[cfg(test)]`:

- **`task_queue.rs`** — single-threaded enqueue/dequeue, concurrent fetching without duplication, exhaustion checks
- **`zone.rs`** — acquire/release correctness, contention mutual exclusion, wrong-owner rejection
- **`monitor.rs`** — online status after registration, heartbeat keeping robot alive, timeout detection, idempotent offline marking

Run all tests with:

```bash
cargo test
```

---

## Known Limitations

- **Fairness** — `try_acquire()` is non-queueing; under single-zone extreme contention, one robot may capture the majority of tasks (observed 93:1 ratio in stress tests). A ticket lock or `Condvar`-based queue would address this.
- **Static zones** — Zones are registered at startup; dynamic addition/removal is not supported.
- **No priority scheduling** — All tasks are treated equally; emergency tasks cannot preempt routine ones.
- **Polling** — Both task fetching and zone retries use fixed-interval sleep rather than condition variables. Adequate at this scale, but would not hold under very high robot counts.

---

## Academic Context

This project was developed for **COMP2432 Operating Systems** at The Hong Kong Polytechnic University (Academic Year 2025–2026) as Project B, demonstrating:

- **Concurrency control** — safe shared state access across multiple threads
- **Synchronization** — preventing race conditions and inconsistent state
- **Coordination** — assigning tasks, enforcing zone exclusion, and detecting failures

---

## References

Key references used in the design and evaluation of this system:

- Klabnik, S., & Nichols, C. (2018). *The Rust Programming Language*. No Starch Press.
- Tanenbaum, A. S., & Bos, H. (2024). *Modern Operating Systems*. Pearson.
- Rodriguez, A., & Osborn, W. (2025). Distributed locking: Performance analysis and optimization strategies. arXiv.
- Kode, O., & Oyemade, T. (2024). Analysis of synchronization mechanisms in operating systems. arXiv.

See the full report for a complete reference list.
