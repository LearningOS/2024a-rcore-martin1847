//! Conditian variable

use crate::sync::{Mutex, UPSafeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// Condition variable structure
/// 有点类似java的wait/notify
pub struct Condvar {
    /// Condition variable inner
    pub inner: UPSafeCell<CondvarInner>,
}

pub struct CondvarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Condvar {
    /// Create a new condition variable
    pub fn new() -> Self {
        trace!("kernel: Condvar::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(CondvarInner {
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// Signal a task waiting on the condition variable
    /// 唤醒丢失 问题 。也就是说和信号量不同，如果调用 signal 的时候没有任何线程在条件变量的阻塞队列中，
    /// 那么这次 signal 不会有任何效果，这次唤醒也不会被记录下来。
    pub fn signal(&self) {
        let mut inner = self.inner.exclusive_access();
        if let Some(task) = inner.wait_queue.pop_front() {
            // 取消Blocked标记，重新改为Ready
            wakeup_task(task);
        }
    }

    /// blocking current task, let it wait on the condition variable
    /// 
    /// // user/src/bin/condsync_condvar.rs
    // const CONDVAR_ID: usize = 0;
    // const MUTEX_ID: usize = 0;

    // unsafe fn first() -> ! {
    //     sleep(10);
    //     println!("First work, Change A --> 1 and wakeup Second");
    //     mutex_lock(MUTEX_ID);
    //     A = 1;
    //     condvar_signal(CONDVAR_ID);
    //     mutex_unlock(MUTEX_ID);
    //     exit(0)
    // }

    // unsafe fn second() -> ! {
    //     println!("Second want to continue,but need to wait A=1");
    //     mutex_lock(MUTEX_ID);
    //     while A == 0 {
    //         println!("Second: A is {}", A);
    //         condvar_wait(CONDVAR_ID, MUTEX_ID);
    //     }
    //     println!("A is {}, Second can work now", A);
    //     mutex_unlock(MUTEX_ID);
    //     exit(0)
    // }
    // create condvar & mutex
    // assert_eq!(condvar_create() as usize, CONDVAR_ID);
    // assert_eq!(mutex_blocking_create() as usize, MUTEX_ID);
    pub fn wait(&self, mutex: Arc<dyn Mutex>) {
        trace!("kernel: Condvar::wait_with_mutex");
        // 条件等待，拿到锁但是不满足的情况下，释放已有资源。见用户端用法
        // user/src/bin/condsync_condvar.rs
        mutex.unlock();
        let mut inner = self.inner.exclusive_access();
        inner.wait_queue.push_back(current_task().unwrap());
        drop(inner);
        // Blocked标记，切走
        block_current_and_run_next();
        mutex.lock();
    }
}
