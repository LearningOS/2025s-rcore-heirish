//! Types related to task management

use super::TaskContext;
use crate::config::MAX_SYSCALL_ID;
/// The task control block (TCB) of a task.

#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// record tasks historical syscall number
    pub task_syscalls: [usize; MAX_SYSCALL_ID + 1],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
