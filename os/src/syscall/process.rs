//! Process management syscalls

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE}, mm::{current_user_table, translated_va_to_pa, MapPermission, MemorySet, VirtPageNum}, task::{
        change_program_brk, current_task, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus
    }, timer::{get_time_ms, get_time_us}
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
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let pa = translated_va_to_pa(current_user_token(),_ts as usize);
    let ts = pa.0 as *mut TimeVal;
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
    // -1
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");

    // debug!("kernel TaskInfo {:?}", _ti);
    let curr_ms = get_time_ms();
    let task = crate::task::current_task();
    let pa = translated_va_to_pa(current_user_token(),_ti as usize).0 as *mut TaskInfo;
    let ti = unsafe { pa.as_mut().unwrap() };
    ti.time = curr_ms - task.running_at_ms;
    ti.status = TaskStatus::Running;

    unsafe {
        core::ptr::copy_nonoverlapping(
            task.syscall_times.as_ptr(),
            ti.syscall_times.as_mut_ptr(),
            task.syscall_times.len(),
        )
    };
    0

    // -1
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if len == 0 {
        trace!("kernel: len 不可为0 !");
        return -1;
    }
    if port & !0x7 != 0 {
        trace!("kernel: port 其余位必须为0 : {}!",port);
        return -1;
    }
    if port & 0x7 == 0 {
        trace!("kernel: port 这样的内存无意义,不可读 : {}!",port);
        return -1;
    }
    if start & (PAGE_SIZE - 1) != 0 {
        trace!("kernel: start 没有按页大小对齐 : {}!",start);
        return -1;
    }

    // -1
    let pages = (len - 1 + PAGE_SIZE)/PAGE_SIZE;
    let table = current_user_table();
    let vpn_start = start/PAGE_SIZE;
    for i in 0..pages {
        let vpn = VirtPageNum(vpn_start + i);
        if table.find_pte(vpn).is_some() {
            trace!("kernel: [start, start + len) 中存在已经被映射的页: {}!",vpn_start + i);
            return -1;
        }
    }

    let permission = MapPermission::from_bits_truncate((port<<1) as u8) | MapPermission::U;

    println!("MMAP permission: {:?}, port {} , pages {} , len {} ",permission,port,pages,len);
    // let pcn =  current_task();
    let mset = &current_task().memory_set as *const MemorySet as *mut MemorySet;
    
    unsafe {
        // (*mset).activate();
        (*mset).insert_framed_area(crate::mm::VirtAddr::from(start), crate::mm::VirtAddr::from(start+len), permission);
    }
    // mset
    0
}

// YOUR JOB: Implement munmap.
// 一定要注意 mmap 是的页表项，注意 riscv 页表项的格式与 port 的区别。
// 你增加 PTE_U 了吗？
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start & (PAGE_SIZE - 1) != 0 {
        trace!("kernel: start 没有按页大小对齐 : {}!",start);
        return -1;
    }

    // -1
    let pages = (len - 1 + PAGE_SIZE)/PAGE_SIZE;
    let table = current_user_table();
    let vpn_start = start/PAGE_SIZE;
    for i in 0..pages {
        let vpn = VirtPageNum(vpn_start + i);
        if table.find_pte(vpn).is_none() {
            trace!("kernel: [start, start + len) 中存在未被映射的虚存: {}!",vpn_start + i);
            return -1;
        }
        println!("==== sys_munmap check VPN {} has pte ",vpn_start + i);
    }

    println!("==== sys_munmap start {} , pages {} , len {} ",start,pages,len);
    
    let mset = &current_task().memory_set as *const MemorySet as *mut MemorySet;
    unsafe {
        (*mset).shrink_to(crate::mm::VirtAddr::from(start), crate::mm::VirtAddr::from(start));
    }
    // mset
    0
    // -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
