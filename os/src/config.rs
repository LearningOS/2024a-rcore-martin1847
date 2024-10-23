//! Constants in the kernel

#[allow(unused)]

/// user app's stack size
pub const USER_STACK_SIZE: usize = 4096 * 2;
/// kernel stack size
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
/// kernel heap size
pub const KERNEL_HEAP_SIZE: usize = 0x200_0000;

/// page size : 4KB
pub const PAGE_SIZE: usize = 0x1000;
/// page size bits: 12
pub const PAGE_SIZE_BITS: usize = 0xc;
/// the max number of syscall
pub const MAX_SYSCALL_NUM: usize = 500;
/// the virtual addr of trapoline
pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
/// the virtual addr of trap context
pub const TRAP_CONTEXT_BASE: usize = TRAMPOLINE - PAGE_SIZE;
/// clock frequency
pub const CLOCK_FREQ: usize = 12500000;
/// the physical memory end
pub const MEMORY_END: usize = 0x88000000;
/// The base address of control registers in Virtio_Block device
/// 内存映射 I/O (MMIO, Memory-Mapped I/O) 指的是外设的设备寄存器可以通过特定的物理内存地址来访问，
/// 每个外设的设备寄存器都分布在没有交集的一个或数个物理地址区间中，不同外设的设备寄存器所占的物理地址空间也不会产生交集
/// 从Qemu for RISC-V 64 平台的 源码 中可以找到 VirtIO 外设总线的 MMIO 物理地址区间为从 0x10001000 开头的 4KiB
/// 后续new_kernel中使用透明的恒等映射，从而让内核可以兼容于直接访问物理地址的设备驱动库。 
pub const MMIO: &[(usize, usize)] = &[(0x10001000, 0x1000)];
