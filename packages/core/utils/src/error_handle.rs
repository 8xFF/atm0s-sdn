use std::fmt::Debug;

pub trait ErrorUtils {
    fn print_error(&self, msg: &str);
}

impl<T, E> ErrorUtils for Result<T, E>
where
    E: Debug,
{
    fn print_error(&self, msg: &str) {
        match self {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error: {} {:?}", msg, e);
            }
        }
    }
}
