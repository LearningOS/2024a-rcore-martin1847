use core::cmp::Ordering;

use crate::mm::StepByOne;

use super::TaskControlBlock;

/// https://rcore-os.cn/rCore-Tutorial-Book-v3/chapter5/5exercise.html#stride
/// the Stride for each TCB
#[derive(Debug)]
pub struct Stride{
    // priority: usize,
    pass: u64,
    step: u64
}

/// 8 位最大255
pub const BIG_STRIDE: u64= u64::MAX;


/// top Priority value, less than this is InValid !
/// direct ratio with run time !! 
pub const MIN_PRIORITY: isize = 2; 

/// 进程初始优先级设置为 16。
pub const DEFAULT_PRIORITY: isize = 16; 


impl Stride {
    /// new Stride with priority, pass = BIG_STRIDE/priority
    pub fn new(priority: isize) -> Self{
        // let prio = prio 
        Self{
            // priority,
            pass: 0,
            step: BIG_STRIDE/(priority as u64)
        }
    }

    /// copy priority/pass from 
    pub fn copy_priority(other:&Stride) -> Self{
        Self{
            pass:0,
            step: other.pass
        }
    }
}

impl Default for Stride {
    fn default() -> Self {
        Stride::new(DEFAULT_PRIORITY)
    }
}

/*
| 进程 | 实际值 (stride) | 理论值 (stride) | 通行值 (pass) |
|------|-----------------|-----------------|---------------|
| A    | 98              | 65634           | 100           |
| B    | 65535           | 65535           | 50            |
u16 A - B = 98 - 65535 = 99  
*/
impl PartialOrd for Stride {

    // 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，
    // 如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。
    // TIPS: 使用 8 bits 存储 stride, BigStride = 255, 则: (125 < 255) == false, (129 < 255) == true
    // https://nankai.gitbook.io/ucore-os-on-risc-v64/lab6/tiao-du-suan-fa-kuang-jia
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // ...
        // 计算两个 pass 的差值
        let diff = self.pass - other.pass;

        // 如果差值在 BigStride / 2 以内，则直接比较
        // 98 - 65535 = -65437 =  99u8
        // 99 < BIG_STRIDE / 2 , then A is bigger😊
        // 65534 - 65535 = -1 = 255u8
        // 255 >  BIG_STRIDE / 2 , then B is bigger😊
        if diff < BIG_STRIDE / 2 {
            // debug!("self step {} pass {} >= other pass {}",self.step,self.pass,other.pass);
            Some(Ordering::Less)
        } else {
            // 否则，反向比较
            // debug!("self pass {} < other pass {}",self.pass,other.pass);
            Some(Ordering::Greater)
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        self.pass ==  other.pass
    }
}

impl StepByOne for Stride {
    fn step(&mut self) {
        self.pass += self.step;
    }
}



impl PartialEq for TaskControlBlock {
    fn eq(&self, other: &Self) -> bool {
        self.inner_readonly_access().stride ==  other.inner_readonly_access().stride
    }
}

impl PartialOrd for TaskControlBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 默认BinaryHeap大顶，咱们换下 other -> self
        other.inner_readonly_access().stride.partial_cmp(&self.inner_readonly_access().stride)
    }
}
impl Eq for TaskControlBlock {
    
}

impl Ord for TaskControlBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}