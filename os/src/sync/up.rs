//! Uniprocessor interior mutability primitives
use core::cell::{RefCell, RefMut};

/// Wrap a static data structure inside it so that we are
/// able to access it without any `unsafe`.
///
/// We should only use it in uniprocessor.
///
/// In order to get mutable reference of inner data, call
/// `exclusive_access`.
pub struct UPSafeCell<T> {
    /// inner data
    inner: RefCell<T>,
}

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

    /// Panic if the data has been borrowed.
    pub fn readonly_access(&self) -> core::cell::Ref<'_, T> {
        self.inner.borrow()
    }

        
    /// manual release refer，then can borrow_mut again.
    pub fn exclusive_release(&self)  {
        // self.inner.()

        // drop(x);

        // 使用 unsafe 访问 borrow 字段
        unsafe {
            // 获取 borrow 字段的引用
            // let borrow_field = &self.inner.borrow;
                    // 获取 RefCell 的裸指针
            let ref_cell_ptr = &self.inner as *const RefCell<T>;

            // 获取 borrow 字段的裸指针
            let borrow_field_ptr: *mut core::cell::Cell<isize> = core::mem::transmute(ref_cell_ptr);

            let curr_borrow_num = (*borrow_field_ptr).get();
            println!("current curr_borrow_num before exclusive_return {}",curr_borrow_num);
            (*borrow_field_ptr).set(curr_borrow_num-1); 
        }
    }
}
