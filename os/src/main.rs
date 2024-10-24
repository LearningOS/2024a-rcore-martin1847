//! The main module and entrypoint
//!
//! Various facilities of the kernels are implemented as submodules. The most
//! important ones are:
//!
//! - [`trap`]: Handles all cases of switching from userspace to the kernel
//! - [`task`]: Task management
//! - [`syscall`]: System call handling and implementation
//! - [`mm`]: Address map using SV39
//! - [`sync`]: Wrap a static data structure inside it so that we are able to access it without any `unsafe`.
//! - [`fs`]: Separate user from file system with some structures
//!
//! The operating system also starts in this module. Kernel code starts
//! executing from `entry.asm`, after which [`rust_main()`] is called to
//! initialize various pieces of functionality. (See its source code for
//! details.)
//!
//! We then call [`task::run_tasks()`] and for the first time go to
//! userspace.

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate log;

extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;

#[macro_use]
mod console;
pub mod config;
pub mod drivers;
pub mod fs;
pub mod lang_items;
pub mod logging;
pub mod mm;
pub mod sbi;
pub mod sync;
pub mod syscall;
pub mod task;
pub mod timer;
pub mod trap;

use core::arch::global_asm;

global_asm!(include_str!("entry.asm"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

#[no_mangle]
/// the rust entry-point of os
pub fn rust_main() -> ! {
    clear_bss();
    println!("[kernel] Hello, world!");
    logging::init();
    mm::init();
    mm::remap_test();
    // 先注册时钟中断处理。。各种信号，中断数组表。
    // Trap::Interrupt(Interrupt::SupervisorTimer) => {
    // riscv 不支持直接设置时钟中断的间隔，只能在每次触发时钟中断的时候，设置下一次时钟中断的时间。
    //     set_next_trigger();
    //     check_timer();
    //     suspend_current_and_run_next();
    // }
    // TrapMode::Direct, 都到base，程序里处理
    trap::init();
    //  通过将 mie 寄存器的 STIE 位（第 5 位）设为 1 开启了内核态的时钟中断。
    // core::arch::asm!("csrrs x0, {1}, {0}",in(reg)bits,const 0x104), 
    //  _set((1<<5));CSR 寄存器 0x104： RISC-V 架构中的 stimecmp 寄存器。这个寄存器用于设置下一个定时器中断的时间点。当系统时钟达到或超过 stimecmp 寄存器中的值时，会触发一个定时器中断。
    trap::enable_timer_interrupt();
    // 定时器在操作系统中非常重要，用于实现时间片轮转调度、定时任务、超时处理等功能。
    timer::set_next_trigger();
    fs::list_apps();
    task::add_initproc();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}
