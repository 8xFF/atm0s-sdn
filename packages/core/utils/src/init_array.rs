#[macro_export]
macro_rules! init_array (
        ($ty:ty, $len:expr, $val:expr) => (
            {
                let mut array: [$ty; $len] = unsafe { std::mem::uninitialized() };
                for i in array.iter_mut() {
                    unsafe { ::std::ptr::write(i, $val); }
                }
                array
            }
        )
);

pub use init_array;