//! Types related to task management & Functions for completely changing TCB

use super::id::TaskUserRes;
use super::{kstack_alloc, KernelStack, ProcessControlBlock, TaskContext};
use crate::sync::ThreadId;
use crate::trap::TrapContext;
use crate::{mm::PhysPageNum, sync::UPSafeCell};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::RefMut;

/// Task control block structure , here is Thread 
/// 用户态线程/协程管理 （只能等当前运行的线程主动让出处理器使用权后，
/// 线程管理运行时才能切换检查，协：yield,主动释放）
/// 根本原因，没有硬件提供的set_timer中断支持Interrupt::SupervisorTimer
/// 大致原理相同，内核态扩展一下对线程的管理，那就可以基于时钟中断来直接打断当前用户态线程的运行，实现对线程的调度和切换等。
/// https://rcore-os.cn/rCore-Tutorial-Book-v3/chapter8/1thread.html
/// https://github.com/cfsamson/example-greenthreads
/// 以下为用户态：GreenThreadManager
/// 在线程向量中查找一个状态为 Ready 的线程控制块；
/// 把当前运行的线程的状态改为 Ready ，把新就绪线程的状态改为 Running 
/// 把 runtime 的 current 设置为这个新线程控制块的id；
/// 调用汇编代码写的函数 asm_switch ，完成两个线程的栈和上下文的切换；
/// 注意到切换线程控制块的函数 t_yield 已经完成了当前运行线程的 state ， id 这两个部分，还缺少：当前指令指针(PC)、通用寄存器集合和栈。
/// asm_switch 主要完成的就是完成这剩下的三部分的切换。
/// TaskContext 
/// x1: u64,  //ra: return address，即当前正在执行线程的当前指令指针(PC)
/// 保存 x1..x27寄存器 和 nx1 u64, //new return address, 即下一个要执行线程的当前指令指针(PC)
///    #[naked]
// #[inline(never)]
// unsafe fn asm_switch(old: *mut TaskContext, new: *const TaskContext) {
//     // a0: old, a1: new
//     llvm_asm!("
//         //if comment below lines: sd x1..., ld x1..., TASK2 can not finish, and will segment fault
//         sd x1, 0x00(a0)
//         sd x2, 0x08(a0)
//         sd x8, 0x10(a0)
//         sd x9, 0x18(a0)
//         sd x18, 0x20(a0) # sd x18..x27
//         ...
//         sd x27, 0x68(a0)
//         sd x1, 0x70(a0)

//         ld x1, 0x00(a1)
//         ld x2, 0x08(a1)
//         ld x8, 0x10(a1)
//         ld x9, 0x18(a1)
//         ld x18, 0x20(a1) #ld x18..x27
//         ...
//         ld x27, 0x68(a1)
//         ld t0, 0x70(a1)
//         //恢复切换后要执行协程的函数返回地址，即 ra 寄存器到 t0 寄存器，然后调用 jr t0 即完成了函数的返回。
//         jr t0
//     "
//     :    :    :    : "volatile", "alignstack"
//     );
// }
// 用户态创建协程
// fn spawn(&mut self, f: fn()) {
// let size = available.stack.len();
// unsafe {
//     let s_ptr = available.stack.as_mut_ptr().offset(size as isize);
//     let s_ptr = (s_ptr as usize & !7) as *mut u8;

//     available.ctx.x1 = guard as u64;  //ctx.x1  is old return address
//     available.ctx.nx1 = f as u64;     //ctx.nx1 is new return address
//     available.ctx.x2 = s_ptr.offset(-32) as u64; //cxt.x2 is sp 增长协程栈

// }
pub struct TaskControlBlock {
    // pub tid : ThreadId,
    /// immutable
    pub process: Weak<ProcessControlBlock>,
    /// Kernel stack corresponding to PID
    /// 每个线程有自己的线程栈 （用户态、内核态）TRAMPOLINE 下面一个内存地址
    /// 每个线程也一个内核栈，不可变
    pub kstack: KernelStack,
    /// mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }

    /// get the thread id
    pub fn tid(&self) -> Option<ThreadId> {
        let inner =  self.inner.exclusive_access();
        inner.res.as_ref().map(|u|u.tid)
    }
}

pub struct TaskControlBlockInner {
    /// 任务（线程）用户态资源
    pub res: Option<TaskUserRes>,
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,
    /// Save task context
    /// 线程之间并不能严格做到隔离。举例来说，一个线程访问另一个线程的栈这种行为并不会被操作系统和硬件禁止。
    /// 这也体现了线程和进程的不同：线程的诞生是为了方便共享，而进程更强调隔离。
    /// 1. 有程序计数器寄存器来记录当前的执行位置
    /// 2. 有一组通用寄存器记录当前的指令的操作数据
    /// 3. 有一个栈作为线程执行过程的函数调用栈保存局部变量等内容，这就形成了线程上下文的主体部分
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,
    /// It is set when active exit or execution error occurs
    pub exit_code: Option<i32>,

    /// 分配矩阵 semp -> cnt
    pub allocation_semaps: Vec<isize>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    #[allow(unused)]
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
}

impl TaskControlBlock {
    /// Create a new task
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                    allocation_semaps:Vec::new()
                })
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
/// The execution status of the current process
pub enum TaskStatus {
    /// ready to run
    Ready,
    /// running
    Running,
    /// blocked
    Blocked,
}
