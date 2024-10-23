use super::BlockDevice;
use crate::mm::{
    frame_alloc, frame_dealloc, kernel_token, FrameTracker, PageTable, PhysAddr, PhysPageNum,
    StepByOne, VirtAddr,
};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
/// 由于设备驱动的开发过程比较琐碎，我们这里直接使用已有的 virtio-drivers crate 
/// 它已经支持 VirtIO 总线架构下的块设备、网络设备、GPU 等设备
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};

/// The base address of control registers in Virtio_Block device
#[allow(unused)]
const VIRTIO0: usize = 0x10001000;
/// VirtIOBlock device driver strcuture for virtio_blk device
pub struct VirtIOBlock(UPSafeCell<VirtIOBlk<'static, VirtioHal>>);

lazy_static! {
    static ref QUEUE_FRAMES: UPSafeCell<Vec<FrameTracker>> = unsafe { UPSafeCell::new(Vec::new()) };
}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .exclusive_access()
            .read_block(block_id, buf)
            .expect("Error when reading VirtIOBlk");
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .exclusive_access()
            .write_block(block_id, buf)
            .expect("Error when writing VirtIOBlk");
    }
}

impl VirtIOBlock {
    #[allow(unused)]
    /// Create a new VirtIOBlock driver with VIRTIO0 base_addr for virtio_blk device
    /// 直接使用MMIO的物理地址
    pub fn new() -> Self {
        unsafe {
            Self(UPSafeCell::new(
                VirtIOBlk::<VirtioHal>::new(&mut *(VIRTIO0 as *mut VirtIOHeader)).unwrap(),
            ))
        }
    }
}

pub struct VirtioHal;

impl Hal for VirtioHal {
    /// VirtIO 设备需要占用部分内存作为一个公共区域从而更好的和 CPU 进行合作。
    /// 这就像 MMU 需要在内存中保存多级页表才能和 CPU 共同实现分页机制一样。
    /// 在 VirtIO 架构下，需要在公共区域中放置一种叫做 VirtQueue 的环形队列，CPU 可以向此环形队列中向 VirtIO 设备提交请求，也可以从队列中取得请求的结果，详情可以参考 virtio 文档 
    /// 对于 VirtQueue 的使用涉及到物理内存的分配和回收，但这并不在 VirtIO 驱动 virtio-drivers 的职责范围之内，因此它声明了数个相关的接口，需要库的使用者自己来实现：
    fn dma_alloc(pages: usize) -> usize {
        let mut ppn_base = PhysPageNum(0);
        // virtio_dma_alloc/dealloc 需要分配/回收数个 连续 的物理页帧，
        // 而我们的 frame_alloc 是逐个分配，严格来说并不保证分配的连续性。
        // 幸运的是，这个过程只会发生在内核初始化阶段，因此能够保证连续性。
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                ppn_base = frame.ppn;
            }
            assert_eq!(frame.ppn.0, ppn_base.0 + i);
            //  FrameTracker 都会被保存在全局的向量 QUEUE_FRAMES 以延长它们的生命周期，避免提前被回收
            QUEUE_FRAMES.exclusive_access().push(frame);
        }
        let pa: PhysAddr = ppn_base.into();
        pa.0
    }

    fn dma_dealloc(pa: usize, pages: usize) -> i32 {
        let pa = PhysAddr::from(pa);
        let mut ppn_base: PhysPageNum = pa.into();
        for _ in 0..pages {
            frame_dealloc(ppn_base);
            ppn_base.step();
        }
        0
    }

    fn phys_to_virt(addr: usize) -> usize {
        addr
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        PageTable::from_token(kernel_token())
            .translate_va(VirtAddr::from(vaddr))
            .unwrap()
            .0
    }
}
