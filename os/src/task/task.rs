//! Types related to task management & Functions for completely changing TCB
use super::stride::Stride;
use super::TaskContext;
use super::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
use crate::config::{MAX_SYSCALL_NUM, TRAP_CONTEXT_BASE};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MemorySet, PhysPageNum, StepByOne, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

/// Task control block structure
///
/// Directly save the contents that will not change during running
pub struct TaskControlBlock {
    // Immutable
    /// Process identifier
    pub pid: PidHandle,

    /// Kernel stack corresponding to PID
    pub kernel_stack: KernelStack,

    // /// schedule priority, min = 2, small has more opportunity
    // pub priority: usize,

    /// Mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let inner = self.inner_exclusive_access();
        inner.memory_set.token()
    }

    /// Get the const reference of the inner TCB
    pub fn inner_readonly_access(&self) -> core::cell::Ref<'_, TaskControlBlockInner>{
        self.inner.readonly_access()
    }
}

pub struct TaskControlBlockInner {
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,

    /// Application data can only appear in areas
    /// where the application address space is lower than base_size
    pub base_size: usize,

    /// Save task context
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,

    /// Application address space
    pub memory_set: MemorySet,

    /// Parent process of the current process.
    /// Weak will not affect the reference count of the parent
    pub parent: Option<Weak<TaskControlBlock>>,

    /// A vector containing TCBs of all child processes of the current process
    pub children: Vec<Arc<TaskControlBlock>>,

    /// It is set when active exit or execution error occurs
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,

    /// Heap bottom
    pub heap_bottom: usize,

    /// Program break
    pub program_brk: usize,

    /// The first time running at , in milliseconds
    pub running_at_ms : usize,
    
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],

    /// stride ,schedule times * pass for Stride
    pub stride: Stride
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
    
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    
    /// mark current task as running, and inc the schedule_time
    pub fn mark_running(&mut self)  {
        self.task_status = TaskStatus::Running;
        if self.running_at_ms == 0 {
            self.running_at_ms = crate::timer::get_time_ms();
        }
        self.stride.step();
        // self.stride
    }
}

impl TaskControlBlock {
    /// Create a new process
    ///
    /// At present, it is only used for the creation of initproc
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            // priority: super::stride::TOP_PRIORITY,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    running_at_ms : 0,
                    stride:Stride::default(),
                    syscall_times: [0; MAX_SYSCALL_NUM]
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// Load a new elf to replace the original application address space and start execution
    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();

        // **** access current TCB exclusively
        let mut inner = self.inner_exclusive_access();
        // substitute memory_set
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize trap_cx
        let trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        *inner.get_trap_cx() = trap_cx;
        // **** release current PCB
    }


    /// parent process fork the child process,with trap_cx_ppn and init MemorySet
    pub fn fork_with(self: &Arc<Self>,trap_cx_ppn:PhysPageNum,memory_set:MemorySet) -> Arc<Self> {
        // ---- access parent PCB exclusively
        // let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        // 跟exec区别，一个来自ELF，一个直接复制地址空间
        // let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        // // warn!(" [fork !!] not OK!! use parent_inner to find trap_cx_ppn!!!");
        // let trap_cx_ppn = memory_set
        //     .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
        //     .unwrap()
        //     .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        

        let mut parent_inner = self.inner_exclusive_access();
        
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }

        // let parent_tcb_inner = self.inner;

        // debug!(" [ fork_with ] set trap_cx.kernel_sp to tcb.task_cx.sp : {}!",kernel_stack_top);
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            // priority:self.priority,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    // 子进程的 Trap 上下文也是完全从父进程复制过来的，
                    // 这可以保证子进程进入用户态和其父进程回到用户态的那一瞬间 CPU 的状态是完全相同的
                    trap_cx_ppn,
                    // 让子进程和父进程的 base_size ，也即应用数据的大小保持一致；
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    // 将父进程的弱引用计数放到子进程的进程控制块中
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    running_at_ms : 0,
                    stride : Stride::copy_priority(&parent_inner.stride),
                    syscall_times: [0; MAX_SYSCALL_NUM]
                })
            },
        });
        // add child
        // 将子进程插入到父进程的孩子向量 children 中。
        parent_inner.children.push(task_control_block.clone());
        task_control_block
    }


    /// parent process fork the child process
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // ---- access parent PCB exclusively
        // let parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        // 跟exec区别，一个来自ELF，一个直接复制地址空间
        // 及时释放exclusive_access
        let memory_set = MemorySet::from_existed_user(&self.inner_exclusive_access().memory_set);
        // warn!(" [fork !!] not OK!! use parent_inner to find trap_cx_ppn!!!");
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        let child_tcb = self.fork_with(trap_cx_ppn, memory_set);
        // modify kernel_sp in trap_cx
        // **** access child PCB exclusively
        let tcb_ptr = child_tcb.clone();
        let tcb = tcb_ptr.inner_exclusive_access();
        let trap_cx = tcb.get_trap_cx();
        // trap_cx.kernel_sp = kernel_stack_top;
        // debug!(" [ fork ] set trap_cx.kernel_sp to tcb.task_cx.sp : {}!",tcb.task_cx.sp);
        trap_cx.kernel_sp = tcb.task_cx.sp;
        // return
        child_tcb
        // **** release child PCB
        // ---- release parent PCB
    }

    /// get pid of process
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    /// change the location of the program break. return None if failed.
    pub fn change_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner_exclusive_access();
        let heap_bottom = inner.heap_bottom;
        let old_break = inner.program_brk;
        let new_brk = inner.program_brk as isize + size as isize;
        if new_brk < heap_bottom as isize {
            return None;
        }
        let result = if size < 0 {
            inner
                .memory_set
                .shrink_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        } else {
            inner
                .memory_set
                .append_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        };
        if result {
            inner.program_brk = new_brk as usize;
            Some(old_break)
        } else {
            None
        }
    }

}

#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Zombie,
}
