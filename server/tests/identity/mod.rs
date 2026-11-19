#![allow(dead_code)]
extern crate alloc;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static FAIL_ALLOCATION: Cell<bool> = const { Cell::new(false) };
}

struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if FAIL_ALLOCATION.try_with(Cell::get).unwrap_or(false) {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if FAIL_ALLOCATION.try_with(Cell::get).unwrap_or(false) {
            std::ptr::null_mut()
        } else {
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn without_allocating<T>(operation: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL_ALLOCATION.set(false);
        }
    }
    FAIL_ALLOCATION.set(true);
    let reset = Reset;
    let result = operation();
    drop(reset);
    result
}

pub mod book {
    include!("../../src/system/identity/book/mod.rs");

    mod tests {
        include!("book.rs");
    }
}
mod revision_owner {
    include!("../../src/system/identity/revision.rs");

    mod tests {
        include!("revision.rs");
    }
}

#[path = "../../src/system/identity/service/answer.rs"]
pub mod answer;
pub mod system {
    pub mod identity {
        pub use crate::book;
    }
}
mod dispatch;
