//! Mutex (spin-like and blocking(sleep))

use super::{ResourceId, ThreadId, UPSafeCell};
use crate::sync::banker::{BankerAlgorithm, DeadlockKind};
use crate::sync::DEAD_LOCK_MAYBE;
use crate::task::{current_process, TaskControlBlock};
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
/// 上述的 MutexBlocking(睡眠型互斥锁）、信号量和条件变量的数据结构几乎相同
/// 都会把挂起的线程放到等待队列中。但是它们的具体实现还是有区别的
/// 这需要同学了解这三种同步互斥机制的操作原理，再看看它们的方法对的设计与实现：
/// 互斥锁的lock和unlock、信号量的up和down、条件变量的wait和signal，就可以看到它们的具体区别了。
pub trait Mutex: Sync + Send {
    /// Lock the mutex,
    fn lock(&self) -> isize;
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
/// 底层还是基于 UPSafeCell ，单核状态可用。
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
        }
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) -> isize{
        trace!("kernel: MutexSpin::lock");
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                // 自旋一下，等待下一个ticket
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
                return 0;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        let mut locked = self.locked.exclusive_access();
        *locked = false;
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    /// current Resoure id
    pub id: ResourceId,
    /// inner mutable
    pub inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    // 用户态陷入内核态之后所有（内核态）中断默认被自动屏蔽
    // 目前我们是在单核上，也 不会有多个 CPU 同时执行系统调用的情况 
    // 在这种情况下，内核态的共享数据访问就仍在 UPSafeCell 的框架之内，只要使用它就能保证互斥访问。
    pub locked: bool,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
    // 当前线程拥有者id
    pub owner_tid : Option<ThreadId>
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new(id: ResourceId) -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            id,
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                    owner_tid:None
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) -> isize{
        trace!("kernel: MutexBlocking::lock {} ",DEAD_LOCK_MAYBE);
        let mut inner = self.inner.exclusive_access();
        if inner.locked {

            let curr_task = current_task().unwrap();
            let tid = curr_task.tid().unwrap();

            if current_process().inner_exclusive_access().enable_deadlock {
                warn!(
                    "down try to check is_deadlock_safe after Mutex/Lock  tid {} -> mutex id {}",
                    tid,self.id
                );

                drop(inner);
                let banker = BankerAlgorithm::new(DeadlockKind::ByMutexBlocking,tid,self.id);

               
                if !banker.is_safe() {
                    error!(
                        " BANK ByMutexBlocking DEAD_LOCK_MAYBE Mutex/Lock  tid {} -> mutex id {}",
                        tid,self.id
                    );
                    return DEAD_LOCK_MAYBE;
                }
                inner = self.inner.exclusive_access();
            }

            inner.wait_queue.push_back(curr_task);
            drop(inner);
            // 标记Blocked ，不再参与调度。
            block_current_and_run_next();
        } else {
            inner.locked = true;
            inner.owner_tid = current_task().unwrap().tid()
        }
        0
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            // 取消Blocked标记，重新改为Ready（可调度），等待下次调度
            // 相当于锁接力，用完传给下一个等着的人，让最后一个负责关锁
            mutex_inner.owner_tid = waking_task.tid();
            wakeup_task(waking_task);
        } else {
            // 没人等着用了，关锁
            mutex_inner.owner_tid = None;
            mutex_inner.locked = false;
        }
    }
}
