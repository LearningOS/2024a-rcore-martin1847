//! File and filesystem-related syscalls
use core::any::{Any, TypeId};

use alloc::sync::Arc;
use easy_fs::NAME_LENGTH_LIMIT;

use crate::fs::{link_times, linkat, open_file, OSInode, OpenFlags, Stat, StatMode};
use crate::mm::{translated_byte_buffer, translated_str, translated_va_to_pa, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    // let fd = inner.fd_table[fd];
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    error!("sys_fstat:fd[{}] !!", fd);
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    // TODO 这里简单实现，过滤到 stdin/out/error
    if fd >= inner.fd_table.len() || fd <= 2 {
        error!("sys_fstat:fd {} > [{}] !!", fd, inner.fd_table.len());
        return -1;
    }
    let fd = inner.fd_table[fd].clone();
    // .map(|&f|f.);
    if fd.is_none() {
        error!("sys_fstat:fd is none !");
        return -1;
    }

    drop(inner);

    
  
   
    let fd = fd.unwrap();

    warn!(
        " sys_fstat   eq {} {:?} == {:?}",
        TypeId::of::<Arc<OSInode>>() == fd.type_id(),
        fd.type_id(),
        TypeId::of::<OSInode>()
    );
    
    // TODO 这里不会判断转换为OSInode，直接unsafe了先
    // let inode =  fd.downcast_ref::<OSInode>() ;
    // if  TypeId::of::<OSInode>() == fd.type_id(){

    let os_inode = unsafe { &*(fd.as_ref() as *const _ as *const OSInode) };
    let inner_inode = os_inode.inner_inode().clone();

    let pa = translated_va_to_pa(current_user_token(), st as usize).0 as *mut Stat;
    let st = unsafe { pa.as_mut().unwrap() };
    st.dev = 0;
    st.ino = inner_inode.inode_id as u64;
    st.mode = StatMode::FILE;

    st.nlink = link_times(inner_inode.inode_id);

    0
}

/// YOUR JOB: Implement linkat.
/// 参数：
// olddirfd，newdirfd: 仅为了兼容性考虑，本次实验中始终为 AT_FDCWD (-100)，可以忽略。
// flags: 仅为了兼容性考虑，本次实验中始终为 0，可以忽略。
// oldpath：原有文件路径
// newpath: 新的链接文件路径。
// 说明：
// 为了方便，不考虑新文件路径已经存在的情况（属于未定义行为），除非链接同名文件。
// 返回值：如果出现了错误则返回 -1，否则返回 0。
// 可能的错误
// 链接同名文件。

pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let new_path = translated_str(token, new_name);

    if new_path.len() > NAME_LENGTH_LIMIT {
        warn!(
            "sys_linkat new_name is too long than {} : {}",
            NAME_LENGTH_LIMIT, new_path
        );
        return -1;
    }

    if open_file(new_path.as_str(), OpenFlags::RDONLY).is_some() {
        warn!("sys_linkat new_name is exists {}", new_path);
        return -1;
    }
    let old_path = translated_str(token, old_name);

    if old_path.eq(&new_path) {
        warn!("sys_linkat new_name is same with  exists !!!{}", new_path);
        return -1;
    }
    error!("sys_linkat try {} ->  {}", old_path, new_path);
    let old_os_inode = open_file(old_path.as_str(), OpenFlags::RDONLY);
    if old_os_inode.is_none() {
        warn!("sys_linkat old_name not found {}", old_path);
        return -1;
    }
    let old_os_inode = old_os_inode.unwrap();

    let new_inode = linkat(&new_path, old_os_inode.inner_inode().inode_id);
    // let new_inode = old_os_inode.inner_inode().linkat(&new_path);

    if new_inode.is_some() {
        warn!("sys_linkat new_path success !!!! {}", new_path);
        0
    } else {
        error!("sys_linkat new_path failed {}", new_path);
        -1
    }
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let name = translated_str(token, name);
    crate::fs::unlink(&name)
}
