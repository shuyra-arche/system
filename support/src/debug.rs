//! Program diagnostics through the Kernel debug channel.

pub fn put(message: &str) {
    let bytes = message.as_bytes();
    let _ = kernel_client::call::debug::put(
        kernel_api::address::VirtAddr::new(bytes.as_ptr() as usize),
        bytes.len(),
    );
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::debug::put(&$crate::__format!($($arg)*));
        }
    };
}
