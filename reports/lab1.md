# 多道程序与分时多任务-实验报告

时间： 2024秋
作者： [martin1847](https://github.com/martin1847)

## 实现功能总结
简单来说，对应用程序的执行，增加一些统计、可观测信息。具体增加了启动时间、系统调用次数。
实现细节如下：
这两个信息放到了`TCB`(TaskControlBlock)中,TCB对应一个进程，全局共享（去掉了copy/clone属性）。
* 启动时间：`running_at_ms`封装到了`mark_running`方法中去设置，如果为0就标记为当前时间戳。
被调用的地方有两个，TaskManager中的`run_first_task`和`run_next_task`。
* 系统调用次数：在TaskManager中增加一个`inc_task_sys_call`方法，在`syscall`进入的时间调用进行统计。
最后，在sys_task_info中读取当前任务的信息，复制到`TaskInfo`中即可。


## 简答作业

### 1. 三个`bad_*.rs`行为描述
三个bad程序都是触发了不同的`Exception`,被Trap处理程序杀掉了（这里是运行下一个程序）。
用的RustSBI version 0.4.0-alpha.1 / RustSBI-QEMU Version 0.2.0-alpha.3
* `ch2b_bad_address`：对空指针进行写操作`0x0 as *mut u8`,触发了`StoreFault`
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
* `ch2b_bad_instructions`: 调用`sret`从S特权返回指令，触发`IllegalInstruction`
* `ch2b_bad_register`: 试图`访问S模式CSR`(控制状态寄存器)，操作S态寄存器`sstatus`,也是触发了`IllegalInstruction`

### 2.  trap.S 中两个函数 __alltraps 和 __restore 相关

1. L40：刚进入 __restore 时，a0 代表了什么值。请指出 __restore 的两种使用情景。

`a0`寄存器是传递第一个入参用的，在`trap.S`中可以看到进入`__restore`之前是调用了`call trap_handler`进行处理。
```rust
trap_handler(cx: &mut TrapContext) -> &mut TrapContext
```
这里对用户态上下文进行了一些修改，交给了`__restore`。所以这里的a0是用户态`TrapContext`的地址。
上面这是一种场景，Trap之后从S态返回User态。
第二种是直接运行用户态程序，可以在`batch.rs/run_next_app`中看到：
```rust
//batch.rs/run_next_app
extern "C" { fn __restore(cx_addr: usize); }
unsafe {
    __restore(KERNEL_STACK.push_context(
        TrapContext::app_init_context(APP_BASE_ADDRESS, USER_STACK.get_sp())
    ) as *const _ as usize);
}
```

2. L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
```s
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2
```
这里通过`csrw` (Control and Status Register Write)指令，用来恢复以下`三个CSR`：
* `sstatus`（Supervisor Status Register） 寄存器的值是为了确保在返回到用户态时，特权状态和控制位与用户态一致。包括中断使能、浮点单元状态、虚拟内存配置、以及一些与特权级别相关的标志位。
* `sepc` (Supervisor Exception Program Counter) 寄存器保存了发生异常时的程序计数器（PC）值。恢复sepc，返回到用户态时，程序能够从异常发生的地方继续执行。
* `sscratch`（Supervisor Scratch Register） 临时/暂存寄存器，一般用来暂存栈指针。写入后 sscratch 保存用户栈指针。从而能够正确地继续执行用户态程序。
    
    
3. L50-L56：为何跳过了 x2 和 x4？
```s
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr
```
跳过 x2 和 x4 的是因为它们有特殊的处理方式。
* x2 : 通常用作栈指针（Stack Pointer），记作 sp。这个由编译器自动处理，而且后续的操作依赖sp。
如果随意更改，后续的 ld 或 sd 指令可能会触发 LoadFault 或 StoreFault 异常。导致上下文切换失败、栈溢出或栈下溢等。
* x4 : 线程指针（Thread Pointer），记作 tp,用于支持多线程环境，通常指向当前线程的线程本地存储（TLS）区域。多线程场景下随便修改，可能会导致跟修改x2类似的问题。这里单线程场景貌似不受影响。不过为了安全、习惯，也不要修改。


4. L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？
`csrrw sp, sscratch, sp`
csrrw 是 "Control and Status Register Read and Write" 。
这是`__restore`之前切换内核、用户栈指针sp。交换前sp位于内核态，sscratch暂存了用户态sp。
交换后sp指向用户态sp了（sscratch暂存内核态sp），这样整个用户上下文就基本恢复完成（99%）了，只差最后一步修改特权标识`sret`。

5. __restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
发生在最后一条特权指令`sret`（Supervisor Return）。这是与`ecall`对应的切权指令。
从 Supervisor 特权级别返回到较低的特权级别。这里是从 S 模式返回 U 模式。


6. L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？
`csrrw sp, sscratch, sp`
这是`__alltraps`时切换内核、用户栈指针sp。
这里是开始陷入内核态。跟`__restore`处`L60`相反。
交换后sp指向内核态（sscratch暂存用户态sp），从内核sp中恢复内核上下文`TrapContext`，作为`call trap_handler`的入参,准备继续执行内核态代码了。

7. 从 U 态进入 S 态是哪一条指令发生的？
用户态程序会调用`syscall`,是通过`ecall`（Execution Environment Call）指令进行的。
```rust
// user/src/syscall.rs
pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id
        );
    }
    ret
}
```


## 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

小组内成员：趁风卷、wyswill等

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

rCore文档V3： https://rcore-os.cn/rCore-Tutorial-Book-v3/chapter3/
rCore v0.2.0实现历程与进展： https://mirrors.tuna.tsinghua.edu.cn/tuna/tunight/2019-03-30-rcore-os/slides.pdf

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

