//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    loader::get_app_data_by_name,
    mm::{current_user_table, translated_refmut, translated_str, translated_va_to_pa, MapPermission, MemorySet, VirtPageNum},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, stride::{Stride, MIN_PRIORITY}, suspend_current_and_run_next, TaskStatus
    },
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
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
/// 用户态程序主动释放CPU/退出进程使用
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0 子进程pid = 0
    trap_cx.x[10] = 0; //x[10] is a0 reg
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        // 认这是对于该子进程控制块的唯一一次强引用，即它不会出现在某个进程的子进程向量中，更不会出现在处理器监控器或者任务管理器中
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // 底回收掉它占用的所有资源，包括：内核栈和它的 PID 还有它的应用地址空间存放页表的那些物理页帧等等
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let ts_va = _ts as usize;
    let ts_page_start = ts_va & !(PAGE_SIZE - 1);
    let ts_page_end = ts_page_start + PAGE_SIZE;
    // let ts_end = ts_va + core::mem::size_of::<TimeVal>();

    if ts_va + core::mem::size_of::<TimeVal>() > ts_page_end {
        // TimeVal 结构体跨越了页边界，返回错误
        return -1;
    }

    let pa = translated_va_to_pa(current_user_token(), ts_va);
    let ts = pa.0 as *mut TimeVal;
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
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");

    // debug!("kernel TaskInfo {:?}", _ti);
    let curr_ms = get_time_ms();
    let task = crate::task::current_task().unwrap();
    let task_inner = &task.inner_exclusive_access();
    let pa = translated_va_to_pa(current_user_token(), _ti as usize).0 as *mut TaskInfo;
    let ti = unsafe { pa.as_mut().unwrap() };
    ti.time = curr_ms - task_inner.running_at_ms;
    ti.status = TaskStatus::Running;

    unsafe {
        core::ptr::copy_nonoverlapping(
            task_inner.syscall_times.as_ptr(),
            ti.syscall_times.as_mut_ptr(),
            task_inner.syscall_times.len(),
        )
    };
    0

    // -1
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    // trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if len == 0 {
        warn!("kernel: len  == 0 !");
        return -1;
    }
    if port & !0x7 != 0 {
        warn!("kernel: port mask must be 0 {}!", port);
        return -1;
    }
    if port & 0x7 == 0 {
        warn!("kernel: port not vaild , R = 0 : {}!", port);
        return -1;
    }
    if start & (PAGE_SIZE - 1) != 0 {
        warn!("kernel: start not aligend!  {}!", start);
        return -1;
    }


    // -1
    let pages = (len - 1 + PAGE_SIZE) / PAGE_SIZE;
    let table = current_user_table();
    let vpn_start = start / PAGE_SIZE;
    for i in 0..pages {
        let vpn = VirtPageNum(vpn_start + i);
        // vpn.0
        debug!("sys_mmap: try to mapping vpn: {:?} / pages {}!", vpn, pages);
        if table.translate(vpn).is_some_and(|p|p.is_valid()) {
            warn!(
                "sys_mmap: [start, start + len) already existed mapping !: {:?} !",
                vpn
            );
            return -1;
        }
    }

    let permission = MapPermission::from_bits_truncate((port << 1) as u8) | MapPermission::U;

    debug!(
        "sys_mmap: permission111 {:?}, start {:#x} , pages {} vpn {:?} , len {} ",
        permission,
        start,
        pages,
        crate::mm::VirtAddr::from(start),
        len
    );
    // let pcn =  current_task();
    let task = crate::task::current_task().unwrap();
    // let mut mset = &task.inner_exclusive_access().memory_set;
    let mset = &task.inner_exclusive_access().memory_set as *const MemorySet as *mut MemorySet;

    unsafe {
        // (*mset).activate();
        (*mset).insert_framed_area(
            crate::mm::VirtAddr::from(start),
            crate::mm::VirtAddr::from(start + pages * PAGE_SIZE),
            permission,
        );
    }
    0
}

// YOUR JOB: Implement munmap.
// 一定要注意 mmap 是的页表项，注意 riscv 页表项的格式与 port 的区别。
// 你增加 PTE_U 了吗？
pub fn sys_munmap(start: usize, len: usize) -> isize {
    // trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start & (PAGE_SIZE - 1) != 0 {
        warn!("kernel: start ptr NOT aligend : {}!", start);
        return -1;
    }

    // -1
    let pages = (len - 1 + PAGE_SIZE) / PAGE_SIZE;
    let table = current_user_table();
    let vpn_start = start / PAGE_SIZE;
    for i in 0..pages {
        let vpn = VirtPageNum(vpn_start + i);
        if table.translate(vpn).is_some_and(|p|!p.is_valid()) {
            warn!(
                "kernel: [start, start + len) has unmapped : {}!",
                vpn_start + i
            );
            return -1;
        }
        // debug!("==== sys_munmap check VPN {} has pte ", vpn_start + i);
    }

    debug!(
        "==== UN sys_munmap start {:#x} ,vpn {:?}, pages {}/ len {} ",
        start,
        crate::mm::VirtAddr::from(start),
        pages,
        len
    );

    let task = crate::task::current_task().unwrap();
    // let mut mset = &task.inner_exclusive_access().memory_set;
    let mset = &task.inner_exclusive_access().memory_set as *const MemorySet as *mut MemorySet;
    unsafe {
        (*mset).shrink_to(
            crate::mm::VirtAddr::from(start),
            crate::mm::VirtAddr::from(start + (pages - 1) * PAGE_SIZE),
        );
    }
    // mset
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

// const EMPTY_MSET:MemorySet = ;

// lazy_static::lazy_static! {
//     static ref EMPTY_MSET_MANUALLY_DROP: core::mem::ManuallyDrop<MemorySet> = 
//     core::mem::ManuallyDrop::new(MemorySet::new_bare());
// }
/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
/// syscall ID: 400
// 功能：新建子进程，使其执行目标程序。
// 说明：成功返回子进程id，否则返回 -1。
pub fn sys_spawn(path: *const u8) -> isize {

    // let token = ;
    let path = translated_str(current_user_token(), path);
    let elf_data =  get_app_data_by_name(path.as_str());
    if elf_data.is_none() {
        debug!("[ spawn ] app {} not found!",path);
        return -1;
    }

    let elf_data = elf_data.unwrap();

    let current_task = current_task().unwrap();
    // spawn 不必 像 fork 一样复制父进程的地址空间。
    // 被替换为ELF，留个站位符即可 
    debug!("[ spawn ] use empty trap_cx_ppn /  MemorySet");
    let new_task = current_task.fork_with(0.into(),MemorySet::new_bare());

    // let n_pid = &new_task.pid;

    // let new_pid = new_task.pid.0;
    new_task.exec(elf_data);
    // modify trap context of new_task, because it returns immediately after switching
    // let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // // we do not have to move to next instruction since we have done it before
    // // for child process, fork returns 0
    // trap_cx.x[10] = 0;  //x[10] is a0 reg
    // add new task to scheduler

    let n_pid = new_task.pid.0;
    add_task(new_task);
    n_pid as isize

    // let pid = sys_fork();

    // 用户态写法
    // if pid == 0 {
    //     // child process
    // }

    // let task = current_task().unwrap();
    // task.exec(app_elf);
    // 0

    // trace!(
    //     "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
    //     current_task().unwrap().pid.0
    // );
    // -1
}

// YOUR JOB: Set task priority.
// syscall ID：140
// 设置当前进程优先级为 prio
// 参数：prio 进程优先级，要求 prio >= 2
// 返回值：如果输入合法则返回 prio，否则返回 -1
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );

    if prio < MIN_PRIORITY {
        return  -1;
    }

    // if prio <=0 || !Stride::is_valid(prio as usize) {
    //     return -1;
    // }
    let current_task = current_task().unwrap();
    let mut task_inner = current_task.inner_exclusive_access();
    task_inner.stride = Stride::new(prio);
    prio
}
