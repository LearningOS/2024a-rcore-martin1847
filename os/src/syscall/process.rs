//! Process management syscalls

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    mm::{current_user_table, write_to_user_virt_target, MapPermission, MemorySet, VirtPageNum},
    task::{
        change_program_brk, current_task, exit_current_and_run_next, suspend_current_and_run_next,
        TaskStatus,
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

// get_time_us TimeVal { sec: 2, usec: 895192 } bytes [2, 0, 0, 0, 0, 0, 0, 0, 216, 168, 13, 0, 0, 0, 0, 0]
// 24000 & 0xFF = 192 , 24000 >> 8 = 93
// TimeVal { sec: 3, usec: 24000 } bytes [3, 0, 0, 0, 0, 0, 0, 0, 192, 93, 0, 0, 0, 0, 0, 0]
// debug!("get_time_us {:?} bytes {:?} ",res,bytes);
// let bytes = unsafe {
//     core::slice::from_raw_parts(
//         &res as *const _ as *const u8,
//         core::mem::size_of::<TimeVal>(),
//     )
// };
// write_to_user_virt_target(current_user_token(), bytes, va_ptr as *mut u8);
// let pa = translated_va_to_pa(current_user_token(), ts_va);
// let ts = pa.0 as *mut TimeVal;

// unsafe {
//     *ts = TimeVal {
//         sec: res.sec,
//         usec: res.usec,
//     };
// }
// 0
// -1
// }

//https://ssl.cdn.maodouketang.com/FoyY6KTxcvuwMfIgvnXb-4g3qX5v
// pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
//     let buffers = crate::mm::translated_byte_buffer(
//         crate::task::current_user_token(),
//         ts as *const u8,
//         core::mem::size_of::<TimeVal>(),
//     );
//     let us = get_time_us();
//     let time_val = TimeVal {
//         sec: us / 1_000_000,
//         usec: us % 1_000_000,
//     };
//     let mut time_val_ptr = &time_val as *const _ as *const u8;
//     for buffer in buffers {
//         unsafe {
//             time_val_ptr.copy_to(buffer.as_mut_ptr(), buffer.len());
//             time_val_ptr = time_val_ptr.add(buffer.len());
//         }
//     }
//     0
// }

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(user_ptr: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let info = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    // 优化点：可以直接使用translated_byte_buffer，获取到mut的切片
    // 指针转换为 *const _ as *const u8后可以直接copy_to
    // 参考上面，尤予阳老师解答给的方案。L70
    write_to_user_space_ptr(&info, user_ptr)
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(user_ptr: *mut TaskInfo) -> isize {
    let task = crate::task::current_task();
    let info = TaskInfo {
        status: TaskStatus::Running,
        time: get_time_ms() - task.running_at_ms,
        syscall_times: task.syscall_times,
    };
    warn!("kernel write_to_user_space_ptr {:?}", user_ptr);
    write_to_user_space_ptr(&info, user_ptr)
}

// debug!("kernel TaskInfo {:?}", _ti);

// let va_ptr = _ti as usize;
// let ts_page_start = va_ptr & !(PAGE_SIZE - 1);
// let ts_page_end = ts_page_start + PAGE_SIZE;

// if va_ptr + core::mem::size_of::<TaskInfo>() > ts_page_end {
//     // TaskInfo 结构体跨越了页边界，返回错误
//     error!(" [`TaskInfo`] is splitted by two pages and not the  #[repr(C)] , not support!");
//     return -1;
// }

// let bytes = unsafe {
//     core::slice::from_raw_parts(
//         &info as *const _ as *const u8,
//         core::mem::size_of::<TaskInfo>(),
//     )
// };

// write_to_user_virt_target(current_user_token(), bytes, va_ptr as *mut u8);
// let pa = translated_va_to_pa(current_user_token(), va_ptr).0 as *mut TaskInfo;
// let ti = unsafe { pa.as_mut().unwrap() };
// ti.time = curr_ms - task.running_at_ms;
// ti.status = TaskStatus::Running;

// unsafe {
//     core::ptr::copy_nonoverlapping(
//         task.syscall_times.as_ptr(),
//         ti.syscall_times.as_mut_ptr(),
//         task.syscall_times.len(),
//     )
// };

// this need the repr of T is same in both kernel and use side.
// It is better to use the repr[C] to compatibility with the Linux.
fn write_to_user_space_ptr<T>(src: &T, user_ptr: *mut T) -> isize {
    let bytes = unsafe {
        core::slice::from_raw_parts(src as *const _ as *const u8, core::mem::size_of::<T>())
    };
    write_to_user_virt_target(bytes, user_ptr as *mut u8);
    0
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
        if table.translate(vpn).is_some_and(|p| p.is_valid()) {
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
    let mset = &current_task().memory_set as *const MemorySet as *mut MemorySet;

    unsafe {
        // (*mset).activate();
        (*mset).insert_framed_area(
            crate::mm::VirtAddr::from(start),
            crate::mm::VirtAddr::from(start + pages * PAGE_SIZE),
            permission,
        );
    }
    // mset
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
        if table.translate(vpn).is_some_and(|p| !p.is_valid()) {
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

    let mset = &current_task().memory_set as *const MemorySet as *mut MemorySet;
    unsafe {
        (*mset).shrink_to(
            crate::mm::VirtAddr::from(start),
            crate::mm::VirtAddr::from(start + (pages - 1) * PAGE_SIZE),
        );
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
