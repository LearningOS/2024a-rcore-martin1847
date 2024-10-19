//! Process management syscalls
use core::ptr::copy_nonoverlapping;

use crate::{
    config::MAX_SYSCALL_NUM,
    task::{current_task, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus},
    timer::{get_time_ms, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info ... martin");
    debug!("kernel TaskInfo {:?}", _ti);
    let curr_ms = get_time_ms();
    let task = current_task();
    let ti = unsafe { _ti.as_mut().unwrap() };
    ti.time = curr_ms - task.running_at_ms;
    ti.status = TaskStatus::Running;

    unsafe {
        copy_nonoverlapping(
            task.syscall_times.as_ptr(),
            ti.syscall_times.as_mut_ptr(),
            task.syscall_times.len(),
        )
    };
    0
    //-1
}
