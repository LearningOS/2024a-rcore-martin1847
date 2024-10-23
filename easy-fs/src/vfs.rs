
use crate::NAME_LENGTH_LIMIT;

use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::{error, warn};
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
/// 对于单个文件的管理和读写的控制逻辑主要是 索引节点（文件控制块） 来完成
/// 这是文件系统的第五层，最顶层，其核心是 Inode 数据结构及其关键成员函数
/// Inode.new：在磁盘上的文件系统中创建一个inode
/// Inode.find：根据文件名查找对应的磁盘上的inode
/// Inode.create：在根目录下创建一个文件
/// Inode.read_at：根据inode找到文件数据所在的磁盘数据块，并读到内存中
/// Inode.write_at：根据inode找到文件数据所在的磁盘数据块，把内存中数据写入到磁盘数据块中
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
    // link_times: Arc<Mutex<u32>>,
    /// inode_id
    pub inode_id: u32,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
        inode_id: u32,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
            // link_times: Arc::new(Mutex::new(1)),
            inode_id,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    pub fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                // error!(" [easy-fs] found DirEntry {}",name);
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                    inode_id,
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }

    /// Create inode under current inode by name
    fn create_with(&self, name: &str, inode_id: Option<u32>) -> Option<u32> {
        if name.len() > NAME_LENGTH_LIMIT {
            warn!(
                "[ vfs Inode ] create failed , file name too long than {},{}",
                NAME_LENGTH_LIMIT, name
            );
            return None;
        }

        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = if inode_id.is_none() {
            let new_inode_id = fs.alloc_inode();
            // initialize inode
            let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
            get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
                .lock()
                .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                    new_inode.initialize(DiskInodeType::File);
                });
                warn!(
                    "[ vfs Inode ]  make new inode id {}",
                    new_inode_id
                );
            new_inode_id
        } else {
            warn!(
                "[ vfs Inode ] is link with old inodeid {}",
                inode_id.unwrap()
            );
            inode_id.unwrap()
        };

        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        Some(new_inode_id)
        // if is_link {
        //     self.link_times +=1;
        //     return Some(Arc::new(self));
        // }

        // None

        // let
    }

    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {

        let new_inode_id = self.create_with(name, None)?;
        let fs = self.fs.lock();
        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
            new_inode_id,
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if dirent.inode_id() > 0 {
                    v.push(String::from(dirent.name()));
                }else{
                    warn!(" [ ls ] found empty DirEntry !!! skip!!")
                }
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    /// linkat
    pub fn link_times(&self,inode_id:u32) -> u32 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut link_times = 0;
            //  warn!(" [ link_times with  file_count =  {}",file_count );
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if dirent.inode_id() == inode_id {
                    warn!(" [ link_times ] found inode_id {} link with {} DirEntry !",self.inode_id ,dirent.name());
                    link_times+=1;
                }
            }
            link_times
        })

    }
    /// linkat
    pub fn linkat(&self, name: &str,inode_id: u32) -> Option<u32> {
        self.create_with(name, Some(inode_id))
        // ?;
        // warn!("[ vfs Inode ] linkat success {} ",self.inode_id);
        // // let _fs = self.fs.lock();
        // // let mut cell = self.link_times.lock();
        // // *cell += 1;
        // // self.link_times.set(self.link_times.get()+1);
        // Some(*cell)
        // self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }

   
    /// Clear the data in current inode
    pub fn clear(&self) {

        let mut fs = self.fs.lock();

        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }


    /// unlink , must from root dir
    pub fn unlink(&self,name: &str) -> isize {
        // ROOT_INODE.find(name).map(|inode| {
        // })   
        let _fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            for i in 0..file_count {
                assert_eq!(
                    disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if dirent.name() == name {
                    disk_inode.write_at(
                        DIRENT_SZ * i,
                        DirEntry::empty().as_bytes(),
                        &self.block_device,
                    );
                    warn!(" [easy-fs] unlink DirEntry {}",name);
                    return 0;
                }
            }
            error!(" [easy-fs] unlink DirEntry  not found !!! {}",name);
            return -1;
        })
    }
}

