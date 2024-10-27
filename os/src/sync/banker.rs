use alloc::collections::btree_map::BTreeMap;

use crate::task::current_process;

use super::{MutexBlocking, ResourceId, ThreadId};

pub enum DeadlockKind {
    #[allow(dead_code)]
    ByMutexBlocking,
    BySemaphore,
}

#[allow(dead_code)]
pub struct BankerAlgorithm {
    // 1 . Available resources
    available_res: BTreeMap<ResourceId, isize>,
    // 2. Allocated resources to each process
    allocation_t2r: BTreeMap<ThreadId, BTreeMap<ResourceId, usize>>,
    // 3. Maximum resource needs of each process
    // 我们这里每次只能申请一个资源，没有就会block，isize都是1了其实，可以退化成BTreeSet
    max_needs_t2r: BTreeMap<ThreadId, BTreeMap<ResourceId, isize>>,
}
#[allow(dead_code)]
impl BankerAlgorithm {
    /// 不做代码入侵，这里在检查的时刻，初始化整个状态矩阵
    pub fn new(kind: DeadlockKind,tid: ThreadId,rid:ResourceId) -> Self {

        let mut available_res = BTreeMap::<ResourceId, isize>::new();
        // 2. Allocated resources to each process
        let mut allocation_t2r = BTreeMap::<ThreadId, BTreeMap<ResourceId, usize>>::new();
        // // 3. Maximum resource needs of each process
        // 我们这里每次只能申请一个资源，没有就会block，isize都是1了其实，可以退化成BTreeSet
        let mut max_needs_t2r = BTreeMap::<ThreadId, BTreeMap<ResourceId, isize>>::new();

        let ps = current_process();
        let psi = ps.inner_exclusive_access();


        max_needs_t2r
                                .entry(tid)
                                .or_insert(BTreeMap::new())
                                .entry(rid)
                                .and_modify(|curr| *curr += 1)
                                .or_insert(1);

        match kind {
            DeadlockKind::ByMutexBlocking => {


                for mx in &psi.mutex_list {
                    if mx.is_none() {
                        continue;
                    }

                    let mx = mx.clone().unwrap();
                    // let mx_ref = .as_ref();
                    let mx = unsafe { &*(mx.as_ref() as *const _ as *const MutexBlocking) };
                    let rid = mx.id;
                    let rinner = mx.inner.readonly_access();
                    available_res.insert(rid, if rinner.locked { 0 } else {1});

                    if let Some(tid) = rinner.owner_tid {
                        allocation_t2r
                            .entry(tid)
                            .or_insert(BTreeMap::new())
                            .insert(rid, 1);
                    }

                    for wait_thread in &rinner.wait_queue {
                        if let Some(tid) = wait_thread.tid() {
                            // 奇怪 wait_quene返回的可能不带tid，也是None
                            // [kernel] Panicked at src/sync/banker.rs:71 called `Option::unwrap()` on a `None` value
                            max_needs_t2r
                                .entry(tid)
                                .or_insert(BTreeMap::new())
                                .entry(rid)
                                .and_modify(|curr| *curr += 1)
                                .or_insert(1);
                        }
                    }

                }
                // Self {
                //     available_res,
                //     // max_needs_t2r: BTreeMap::new(),
                //     allocation_t2r,
                //     max_needs_t2r,
                // }
            }
            DeadlockKind::BySemaphore => {

                for semap in &psi.semaphore_list {
                    if semap.is_none() {
                        continue;
                    }
                    let semap = &semap.clone().unwrap();
                    let rid = semap.id;
                    let rinner = semap.inner.readonly_access();
                    available_res.insert(rid, rinner.count);

                    for (tid, has_cnt) in &rinner.owner_map {
                        allocation_t2r
                            .entry(*tid)
                            .or_insert(BTreeMap::new())
                            .insert(rid, *has_cnt);
                    }

                    for wait_thread in &rinner.wait_queue {
                        if let Some(tid) = wait_thread.tid() {
                            // 奇怪 wait_quene返回的可能不带tid，也是None
                            // [kernel] Panicked at src/sync/banker.rs:71 called `Option::unwrap()` on a `None` value
                            max_needs_t2r
                                .entry(tid)
                                .or_insert(BTreeMap::new())
                                .entry(rid)
                                .and_modify(|curr| *curr += 1)
                                .or_insert(1);
                        }
                    }
                }
             
            }
        };

        Self {
            available_res,
            // max_needs_t2r: BTreeMap::new(),
            allocation_t2r,
            max_needs_t2r,
        }
    }


    pub fn is_safe(&self) -> bool {

        // return  true;

        let mut work = self.available_res.clone();
        let mut finish = BTreeMap::new();

        let ps = current_process();
        let psi = ps.inner_exclusive_access();

        for thread in &psi.tasks {
            if let Some(tid) = thread.as_ref().and_then(|t|t.tid()) {
                // TODO 1.  不全部初始化为false了， 只有 allocation_t2r > 0 的 为false，表示还占有资源
                finish.insert(tid, false);
            }
        }

        loop {
            let mut found = false;

            // O(N^2)
            for (tid, can_finish) in finish.iter_mut() {
                // 遍历没完成的线程，检查 request[i] < work[i],
                // 每个资源检查一遍 总复杂度 O(M^N2)
                if !*can_finish && self.can_finish(tid, &work) {
                    // 回收回来，表示可以完成的，继续找
                    self.release_resources(tid, &mut work);
                    *can_finish = true;
                    found = true;
                }

                // *value += 1; // 修改值
                // println!("Updated key: {}, value: {}", key, value);
            }

            if !found {
                break;
            }
        }

        // 只要一个false那就是死锁
        finish.values().all(|&f| f)
    }

    fn can_finish(&self, tid: &ThreadId, work: &BTreeMap<ResourceId, isize>) -> bool {
        // let need_map = ;
        if let Some(need_map) = self.max_needs_t2r.get(tid){
            for (resource_id, &need) in need_map {
                if need > work[resource_id] {
                    return false;
                }
            }
        }
        true
    }

    fn release_resources(&self, tid: &ThreadId, work: &mut BTreeMap<ResourceId, isize>) {
        if let Some(alloc_map) = self.allocation_t2r.get(tid){
            for (resource_id, &allocated) in alloc_map {
                *work.get_mut(resource_id).unwrap() += allocated as isize;
            }
        }
    }
}
