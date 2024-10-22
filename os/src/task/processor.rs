//!Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
// use alloc::boxed::Box;
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    ///The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,

    ///The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    ///Get current task in moving semanteme
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    ///Get current task in cloning semanteme
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}


// struct MyDropTest(Box<usize>);

// impl  Drop for  MyDropTest {
//     fn drop(&mut self) {
//         warn!("drop test after swtich :: {}",self.0)
//     }
// }

///The main part of process execution and scheduling
///Loop `fetch_task` to get the process that needs to run, and switch the process through `__switch`
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            // __switch 的第一个参数，也就是当前 idle 控制流的 task_cx_ptr
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            // next_task_cx_ptr 作为 __switch 的第二个参数，然后修改任务的状态为 Running 。
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            if task_inner.running_at_ms == 0 {
                task_inner.running_at_ms = get_time_ms();
            }
            // release coming task_inner manually
            // 手动回收对即将执行任务的任务控制块的借用标记，使得后续我们仍可以访问该任务控制块。
            // 这里我们不能依赖编译器在 if let 块结尾时的自动回收，因为中间我们会在自动回收之前调用 __switch
            // 已经结束访问却没有进行回收的情况下切换到下一个任务，最终可能违反 UPSafeCell 的借用约定而使得内核报错退出。
            // 后面线程马上要使用TCB
            
            drop(task_inner);
            // release coming task TCB manually
            // 在稳定的情况下，每个尚未结束的进程的任务控制块都只能被引用一次，
            // 要么在任务管理器中，要么则是在代表 CPU 处理器的 Processor 中。
            processor.current = Some(task);
            // release processor manually
            // warn!("run_tasks we donot drop processor.... here !!");
            drop(processor);
            // PROCESSOR.exclusive_release();
            // let my_drop_test = MyDropTest(Box::new(5));
            // warn!("test new my_drop_test {}",my_drop_test.0);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            warn!("no tasks available in run_tasks");
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the current user token(addr of page table)
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

///Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

///Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

/// inc sys_call for current task , return the times .include this time.
pub fn inc_task_sys_call(syscall_id: usize) {
    if let Some(task) = current_task() {
        // access coming task TCB exclusively
        let mut task_inner = task.inner_exclusive_access();
        task_inner.syscall_times[syscall_id] += 1;
    }
}
