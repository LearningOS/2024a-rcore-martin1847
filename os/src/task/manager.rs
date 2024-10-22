//!Implementation of [`TaskManager`]
// use core::cmp::Ordering;


use super::TaskControlBlock;
use crate::sync::UPSafeCell;
// use alloc::borrow::ToOwned;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        
        let dq = &mut self.ready_queue;
        if dq.is_empty() {
            return  None;
        }

        let mut min_index = 0;
        // let mut max_index = 0;
        for (i, value) in dq.iter().enumerate() {
            if value.inner_readonly_access().stride < dq[min_index].inner_readonly_access().stride {
                min_index = i;
            }
        }
        
        // warn!("found min_index stride {} -> {:?}",min_index,dq.get(min_index).unwrap().inner_readonly_access().stride);
        dq.remove(min_index)
        
        
        // warn!("found min_index stride {:?} / max {:?}, default : {:?}"
        // ,dq.get(min_index).unwrap().inner_readonly_access().stride
        // ,dq.get(max_index).unwrap().inner_readonly_access().stride
        // ,dq.get(0).unwrap().inner_readonly_access().stride
        // );
        // self.ready_queue.pop_front()
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
