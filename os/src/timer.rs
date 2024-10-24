//! RISC-V timer-related functionality

use core::cmp::Ordering;

use crate::config::CLOCK_FREQ;
use crate::sbi::set_timer;
use crate::sync::UPSafeCell;
use crate::task::{current_task, wakeup_task, TaskControlBlock};
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use lazy_static::*;
use riscv::register::time;
/// The number of ticks per second， 10 ms 一次
const TICKS_PER_SEC: usize = 100;
/// The number of milliseconds per second
const MSEC_PER_SEC: usize = 1000;
/// The number of microseconds per second
#[allow(dead_code)]
const MICRO_PER_SEC: usize = 1_000_000;

/// Get the current time in ticks
pub fn get_time() -> usize {
    // [ csrrs rd, csr, rs1 ] 
    // rs1 是源寄存器，用于设置 CSR 的值。如果 rs1 为 x0（零寄存器），则表示不设置 CSR 的值，只读取。
    // let r:usize;
    // core::arch::asm!("csrrs {0}, {1}, x0",out(reg)r,const 0xC01);
    // out(reg) r 表示将读取的值存储到 Rust 变量 r:usize 中。
    // CSR 寄存器 0xC01 是 RISC-V 架构中的 time 寄存器。这个寄存器存储了当前的系统时间（时钟周期数）。
    time::read()
}

/// Get the current time in milliseconds
pub fn get_time_ms() -> usize {
    time::read() * MSEC_PER_SEC / CLOCK_FREQ
}

/// get current time in microseconds
pub fn get_time_us() -> usize {
    time::read() * MICRO_PER_SEC / CLOCK_FREQ
}

/// Set the next timer interrupt
pub fn set_next_trigger() {
// CLOCK_FREQ 是系统的时钟频率，表示每秒钟的时钟周期数。
// TICKS_PER_SEC 是每秒钟希望触发的定时器中断次数。
// TIMEBASE = CLOCK_FREQ / TICKS_PER_SEC 计算出每个定时器中断之间的时间间隔（以时钟周期数为单位）。
// get_time() + CLOCK_FREQ / TICKS_PER_SEC 计算出下一个定时器中断的时间点。
// 调用 SBI 的 SBI_SET_TIMER 服务
// set_next_trigger 能够精确地设置下一个定时器中断的时间点，从而实现高效的时间管理。
// TIMEBASE 便是时间间隔，其数值一般约为 cpu 频率的 1% ，防止时钟中断占用过多的 cpu 资源。
// 这里TICKS_PER_SEC也是按照这个策略，100
    set_timer(get_time() + CLOCK_FREQ / TICKS_PER_SEC);
}

/// condvar for timer
pub struct TimerCondVar {
    /// The time when the timer expires, in milliseconds
    pub expire_ms: usize,
    /// The task to be woken up when the timer expires
    pub task: Arc<TaskControlBlock>,
}

impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        self.expire_ms == other.expire_ms
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = -(self.expire_ms as isize);
        let b = -(other.expire_ms as isize);
        Some(a.cmp(&b))
    }
}

impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

lazy_static! {
    /// TIMERS: global instance: set of timer condvars
    static ref TIMERS: UPSafeCell<BinaryHeap<TimerCondVar>> =
        unsafe { UPSafeCell::new(BinaryHeap::<TimerCondVar>::new()) };
}

/// Add a timer
pub fn add_timer(expire_ms: usize, task: Arc<TaskControlBlock>) {
    trace!(
        "kernel:pid[{}] add_timer",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let mut timers = TIMERS.exclusive_access();
    timers.push(TimerCondVar { expire_ms, task });
}

/// Remove a timer
pub fn remove_timer(task: Arc<TaskControlBlock>) {
    //trace!("kernel:pid[{}] remove_timer", current_task().unwrap().process.upgrade().unwrap().getpid());
    trace!("kernel: remove_timer");
    let mut timers = TIMERS.exclusive_access();
    let mut temp = BinaryHeap::<TimerCondVar>::new();
    for condvar in timers.drain() {
        if Arc::as_ptr(&task) != Arc::as_ptr(&condvar.task) {
            temp.push(condvar);
        }
    }
    timers.clear();
    timers.append(&mut temp);
    trace!("kernel: remove_timer END");
}

/// Check if the timer has expired
pub fn check_timer() {
    trace!(
        "kernel:pid[{}] check_timer",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let current_ms = get_time_ms();
    let mut timers = TIMERS.exclusive_access();
    while let Some(timer) = timers.peek() {
        if timer.expire_ms <= current_ms {
            wakeup_task(Arc::clone(&timer.task));
            timers.pop();
        } else {
            break;
        }
    }
}
