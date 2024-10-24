//! Safe Cell for uniprocessor（single cpu core）
//!
//! UPSafeCell is used to wrap a static data structure which can access safely.
//!
//! NOTICE: We should only use it in environment with uniprocessor（single cpu core）, and the kernel can not support task preempting in kernel mode （or trap in kernel mode）.

use core::cell::{RefCell, RefMut};

/// Wrap a static data structure inside it so that we are
/// able to access it without any `unsafe`.
///
/// We should only use it in uniprocessor.
///
/// In order to get mutable reference of inner data, call
/// `exclusive_access`.
/// 允许在单核上安全使用可变全局变量 
/// only use it in uniprocessor.
/// only use it in uniprocessor.
/// only use it in uniprocessor.
pub struct UPSafeCell<T> {
    /// inner data
    inner: RefCell<T>,
}
// 支持在单核处理器上安全地在线程间共享可变全局变量。
// 因为是单核，所以欺骗/告诉编译器，声明支持全局变量安全地在线程间共享
unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    /// User is responsible to guarantee that inner struct is only used in
    /// uniprocessor.
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }
    /// Panic if the data has been borrowed.
    pub fn exclusive_access(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }
}
