//! Types related to task management

use crate::{config::MAX_SYSCALL_NUM, timer::get_time_ms};

use super::TaskContext;

/// The task control block (TCB) of a task. global share, No need to Copy/Clone
// #[derive(Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The first time running at , in milliseconds
    pub running_at_ms : usize,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
}

impl TaskControlBlock {

    /// Make a TaskControlBlock, mark as UnInit  
    pub fn new()-> TaskControlBlock{
        TaskControlBlock {
            task_cx: TaskContext::zero_init(),
            task_status: TaskStatus::UnInit,
            running_at_ms : 0,
            syscall_times: [0; MAX_SYSCALL_NUM]
        }
    }

    /// Mark the task ro Running Status 
    pub fn mark_running(&mut self){
        self.task_status =  TaskStatus::Running;
        if self.running_at_ms == 0 {
            self.running_at_ms = get_time_ms();
        }
    }

    /// inc once sys call by call id
    pub fn inc_sys_call(&mut self,syscall_id: usize)->u32{
        self.syscall_times[syscall_id] += 1;
        self.syscall_times[syscall_id]
    }

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
