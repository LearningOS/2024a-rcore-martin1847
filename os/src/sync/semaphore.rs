//! Semaphore

use crate::sync::banker::BankerAlgorithm;
use crate::sync::{banker, UPSafeCell, DEAD_LOCK_MAYBE};
use crate::task::{
    block_current_and_run_next, current_process, current_task, wakeup_task, TaskControlBlock,
};

use alloc::collections::btree_map::BTreeMap;
use alloc::{collections::VecDeque, sync::Arc};

use super::{ResourceId, ThreadId};

/// semaphore structure
/// 跟java的pack & unpack很像。不过unpack多次，也只有一个许可。
pub struct Semaphore {
    /// current Resoure id
    pub id: ResourceId,
    /// init num of this Resoure type
    pub total: isize,
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
    // /// some one has get it. 同一线程多次出现，表示占有多个
    pub owner_map: BTreeMap<ThreadId, usize>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize, id: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            id,
            total: res_count as isize,
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                    owner_map: BTreeMap::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();

        // if inner.count == self.total {
        //     panic!(" [ make new resouce is Not Allowd!! ] {}",self.total);
        // }

        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self) -> isize {
        trace!("kernel: Semaphore::down");
        // let cnt = self.inner.readonly_access().count;
        let mut inner = self.inner.exclusive_access();

        // 没有了。准备上锁阶段
        if inner.count < 1 {
            // let enable_deadlock = ;

            let curr_task = current_task().unwrap();
            let tid = curr_task.tid().unwrap();
            let cnt = inner.count;
            inner.wait_queue.push_back(curr_task);

            if current_process().inner_exclusive_access().enable_deadlock {

                let rid = self.id;
                warn!(
                    "down try to check is_deadlock_safe before --count  tid {} -> seampid {} : {}/{}",
                    tid, rid,cnt, self.total
                );

                drop(inner);
                //后面要用
                let banker = BankerAlgorithm::new(banker::DeadlockKind::BySemaphore);
                // 用完了，弄回来
                inner = self.inner.exclusive_access();
                if !banker.is_safe() {
                    error!(
                        " BANK BySemaphore DEAD_LOCK !!! tid {} -> seampid {} ",
                        tid,rid
                    );
                     // 没拿到锁，别等了。
                    inner.wait_queue.pop_back();
                    return DEAD_LOCK_MAYBE;
                }
                
            }

            inner.count -= 1;
            drop(inner);
            
            block_current_and_run_next();
            return 0;
        }
        // let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        // inner.owner_map.
        inner
            .owner_map
            .entry(current_task().unwrap().tid().unwrap())
            .and_modify(|curr| *curr += 1)
            .or_insert(1);
        // *inner.owner_map.entry(current_task().unwrap().tid()).or_insert(0) += 1;
        0
    }

}
