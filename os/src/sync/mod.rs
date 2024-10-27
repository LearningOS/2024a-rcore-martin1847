//! Synchronization and interior mutability primitives

mod condvar;
mod mutex;
mod semaphore;
mod up;

pub use condvar::Condvar;
pub use mutex::{Mutex, MutexBlocking, MutexSpin};
pub use semaphore::Semaphore;
pub use up::UPSafeCell;

const DEAD_LOCK_MAYBE:isize = -0xDEAD;
mod banker;

/// thread id 
pub type ThreadId = usize;
/// Resource Id
pub type ResourceId = usize;