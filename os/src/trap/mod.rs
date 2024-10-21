//! Trap handling functionality
//!
//! For rCore, we have a single trap entry point, namely `__alltraps`. At
//! initialization in [`init()`], we set the `stvec` CSR to point to it.
//!
//! All traps go through `__alltraps`, which is defined in `trap.S`. The
//! assembly language code does just enough work restore the kernel space
//! context, ensuring that Rust code safely runs, and transfers control to
//! [`trap_handler()`].
//!
//! It then calls different functionality based on what exactly the exception
//! was. For example, timer interrupts trigger task preemption, and syscalls go
//! to [`syscall()`].

mod context;

use crate::config::{TRAMPOLINE, TRAP_CONTEXT_BASE};
// use crate::mm::{current_user_table, MemorySet};
use crate::syscall::syscall;
use crate::task::{
    current_trap_cx, current_user_token, exit_current_and_run_next, suspend_current_and_run_next,
};
use crate::timer::set_next_trigger;
use core::arch::{asm, global_asm};
use riscv::register::{
    mtvec::TrapMode,
    scause::{self, Exception, Interrupt, Trap},
    sie, stval, stvec,
};

global_asm!(include_str!("trap.S"));

/// Initialize trap handling
pub fn init() {
    set_kernel_trap_entry();
}

fn set_kernel_trap_entry() {
    unsafe {
        stvec::write(trap_from_kernel as usize, TrapMode::Direct);
    }
}

fn set_user_trap_entry() {
    unsafe {
        stvec::write(TRAMPOLINE as usize, TrapMode::Direct);
    }
}

/// enable timer interrupt in supervisor mode
pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer();
    }
}

/// trap handler
#[no_mangle]
pub fn trap_handler() -> ! {
    //将 stvec 修改为同模块下另一个函数 trap_from_kernel 的地址。这就是说，一旦进入内核后再次触发到 S 的 Trap，
    // 则会在硬件设置一些 CSR 之后跳过寄存器 的保存过程直接跳转到 trap_from_kernel 函数，在这里我们直接 panic 退出
    set_kernel_trap_entry();
    // 由于应用的 Trap 上下文不在内核地址空间，因此我们调用 current_trap_cx 来获取当前应用的 Trap 上下文的可变引用
    // 而不是像之前那样作为参数传入 trap_handler 
    let cx = current_trap_cx();
    let scause = scause::read(); // get trap cause
    let stval = stval::read(); // get extra value
    // trace!("into {:?}", scause.cause());
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            // jump to next instruction anyway
            cx.sepc += 4;
            // get system call return value
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        Trap::Exception(Exception::StoreFault)
        | Trap::Exception(Exception::StorePageFault)
        | Trap::Exception(Exception::LoadFault)
        | Trap::Exception(Exception::LoadPageFault) => {

            // let mset = &crate::task::current_task().memory_set as *const MemorySet as *mut MemorySet;
            // unsafe {
            //     (*mset).
            // }

            println!("[kernel] PageFault in application, bad addr = {:#x}, bad instruction = {:#x}, kernel killed it.", stval, cx.sepc);
            exit_current_and_run_next();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            exit_current_and_run_next();
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_trigger();
            suspend_current_and_run_next();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    //println!("before trap_return");
    trap_return();
}

#[no_mangle]
/// return to user space
/// set the new addr of __restore asm function in TRAMPOLINE page,
/// set the reg a0 = trap_cx_ptr, reg a1 = phy addr of usr page table,
/// finally, jump to new addr of __restore asm function
pub fn trap_return() -> ! {
    // set_user_trap_entry 来让应用 Trap 到 S 的时候可以跳转到 __alltraps 。注意我们把 stvec 设置为内核和应用地址空间共享的跳板页面的起始地址 TRAMPOLINE 
    // 而不是 编译器在链接时看到的 __alltraps 的地址，因为启用分页模式之后我们只能通过跳板页面上的虚拟地址来实际取得 __alltraps 和 __restore 的汇编代码。
    //  stvec::write(TRAMPOLINE as usize, TrapMode::Direct);
    set_user_trap_entry();
    // __restore 需要两个参数：1. Trap 上下文在应用地址空间中的虚拟地址
    // 2. 要继续执行的应用 地址空间的 token 下面两行则分别准备好这两个参数。
    let trap_cx_ptr = TRAP_CONTEXT_BASE;
    let user_satp = current_user_token();
    extern "C" {
        fn __alltraps();
        fn __restore();
    }
    let restore_va = __restore as usize - __alltraps as usize + TRAMPOLINE;
    // trace!("[kernel] trap_return: ..before return");
    unsafe {
        asm!(
            //i-cache 中可能还保存着该物理页帧的 错误快照。因此我们直接将整个 i-cache 清空避免错误。
            "fence.i",
            "jr {restore_va}",         // jump to new addr of __restore asm function
            restore_va = in(reg) restore_va,
            in("a0") trap_cx_ptr,      // a0 = virt addr of Trap Context
            in("a1") user_satp,        // a1 = phy addr of usr page table
            options(noreturn)
        );
    }
}

#[no_mangle]
/// handle trap from kernel
/// Unimplement: traps/interrupts/exceptions from kernel mode
/// Todo: Chapter 9: I/O device
/// 这里简单起见我们弱化了从 S 到 S 的 Trap ，省略了 Trap 上下文保存过程而直接 panic 。
pub fn trap_from_kernel() -> ! {
    use riscv::register::sepc;
    trace!("stval = {:#x}, sepc = {:#x}", stval::read(), sepc::read());
    panic!("a trap {:?} from kernel!", scause::read().cause());
}

pub use context::TrapContext;
