use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::Arc,
};

use crate::task::{current_process, TaskControlBlock};

use super::{MutexBlocking, ResourceId, ThreadId};

pub enum DeadlockKind {
    ByMutexBlocking,
    BySemaphore,
}

#[allow(dead_code)]
pub struct BankerAlgorithm {
    // 1 . Available resources
    available_res_map: BTreeMap<ResourceId, isize>,
    // 2. Allocated resources to each process , matrix
    allocation_t2r_matrix: BTreeMap<ThreadId, BTreeMap<ResourceId, usize>>,
    // 3. Maximum resource  needs of each process , matrix
    // 我们这里每次只能申请一个资源，没有就会block，isize都是1了其实，可以退化成BTreeSet
    still_needs_t2r_matrix: BTreeMap<ThreadId, BTreeMap<ResourceId, isize>>,
}
#[allow(dead_code)]
impl BankerAlgorithm {
    /// 尽量减少代码入侵，这里在检查的时刻，初始化整个状态矩阵
    pub fn new(kind: DeadlockKind) -> Self {
        let mut available_res_map: BTreeMap<ResourceId, isize> = BTreeMap::new();
        let mut allocation_t2r_matrix: BTreeMap<ThreadId, BTreeMap<ResourceId, usize>> =
            BTreeMap::new();
        let mut still_needs_t2r_matrix: BTreeMap<ThreadId, BTreeMap<ResourceId, isize>> =
            BTreeMap::new();

        let ps = current_process();
        let psi = ps.inner_exclusive_access();

        
        fn init_still_needs_matrix(
            still_needs_t2r_matrix: &mut BTreeMap<ThreadId, BTreeMap<ResourceId, isize>>,
            wait_queue: &VecDeque<Arc<TaskControlBlock>>,
            rid: ResourceId,
        ) {
            for wait_thread in wait_queue {
                if let Some(tid) = wait_thread.tid() {
                    BankerAlgorithm::inc_thread_needs(still_needs_t2r_matrix,tid,rid);
                }
            }
        }
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
                    available_res_map.insert(rid, if rinner.locked { 0 } else { 1 });

                    if let Some(tid) = rinner.owner_tid {
                        allocation_t2r_matrix
                            .entry(tid)
                            .or_insert(BTreeMap::new())
                            .insert(rid, 1);
                    }

                    init_still_needs_matrix(&mut still_needs_t2r_matrix, &rinner.wait_queue, rid);
                }
            }
            DeadlockKind::BySemaphore => {
                for semap in &psi.semaphore_list {
                    if semap.is_none() {
                        continue;
                    }
                    let semap = &semap.clone().unwrap();
                    let rid = semap.id;
                    let rinner = semap.inner.readonly_access();
                    available_res_map.insert(rid, rinner.count);

                    for (tid, has_cnt) in &rinner.owner_map {
                        allocation_t2r_matrix
                            .entry(*tid)
                            .or_insert(BTreeMap::new())
                            .insert(rid, *has_cnt);
                    }
                    init_still_needs_matrix(&mut still_needs_t2r_matrix, &rinner.wait_queue, rid);
                }
            }
        };

        Self {
            available_res_map,
            allocation_t2r_matrix,
            still_needs_t2r_matrix,
        }
    }

    pub fn pre_safe_for_need(&mut self,tid: ThreadId,rid: ResourceId) -> bool {
        // return  true;

        let ps = current_process();
        let psi = ps.inner_exclusive_access();

        BankerAlgorithm::inc_thread_needs(&mut self.still_needs_t2r_matrix,tid,rid);

        let mut work: &mut BTreeMap<ResourceId, isize> = &mut self.available_res_map;
        let mut finish_map: BTreeMap<ThreadId, bool> = BTreeMap::new();

        for thread in &psi.tasks {
            if let Some(tid) = thread.as_ref().and_then(|t| t.tid()) {
                // 不全部初始化为false了， 只有 allocation_t2r > 0 的 为false，表示还占有资源
                //  allow_finish : resoure == None || is_empty
                let allow_finish = self
                    .allocation_t2r_matrix
                    .get(&tid)
                    .map(|r| r.is_empty())
                    .unwrap_or(true);
                // warn!(" tid {} has allocations , allow_finish : {}",tid,allow_finish);
                finish_map.insert(tid, allow_finish);
            }
        }

        //     // 内联的方法，避免外部借用问题
        // fn can_finish(&self, tid: &ThreadId, work: &BTreeMap<ResourceId, isize>) -> bool {
        //     let needs = self.still_needs_t2r_matrix.get(tid).unwrap_or(&BTreeMap::new());
        //     needs.iter().all(|(&rid, &need)| {
        //         work.get(&rid).map_or(true, |&avail| avail >= need)
        //     })
        // }
        
        loop {
            let mut found = false;

            // O(N^2)
            for (tid, can_finish) in finish_map.iter_mut() {
                // 遍历没完成的线程，检查 request[i] < work[i],
                // 每个资源检查一遍 总复杂度 O(M^N2)
                if !*can_finish && BankerAlgorithm::can_finish(&self.still_needs_t2r_matrix,tid, &work) {
                    // 回收回来，表示可以完成的，继续找
                    BankerAlgorithm::release_resources(&self.allocation_t2r_matrix,tid, &mut work);
                    *can_finish = true;
                    found = true;
                }
            }

            if !found {
                break;
            }
        }

        // 只要一个false那就是死锁
        finish_map.values().all(|&f| f)
    }

    fn can_finish(still_needs_t2r_matrix: &BTreeMap<ThreadId, BTreeMap<ResourceId, isize>>
        , tid: &ThreadId, work: &BTreeMap<ResourceId, isize>) -> bool {
        // let need_map = ;
        if let Some(need_map) = still_needs_t2r_matrix.get(tid) {
            for (resource_id, &need) in need_map {
                if need > work[resource_id] {
                    return false;
                }
            }
        }
        true
    }

    fn release_resources(allocation_t2r_matrix:&BTreeMap<ThreadId, BTreeMap<ResourceId, usize>>, tid: &ThreadId, work: &mut BTreeMap<ResourceId, isize>) {
        if let Some(alloc_map) = allocation_t2r_matrix.get(tid) {
            for (resource_id, &allocated) in alloc_map {
                *work.get_mut(resource_id).unwrap() += allocated as isize;
            }
        }
    }


    fn inc_thread_needs(
        still_needs_t2r_matrix: &mut BTreeMap<ThreadId, BTreeMap<ResourceId, isize>>,
        tid: ThreadId,
        rid: ResourceId,
    ) {
        still_needs_t2r_matrix
            .entry(tid)
            .or_insert_with(BTreeMap::new)
            .entry(rid)
            .and_modify(|curr| *curr += 1)
            .or_insert(1);
    }

    
}
