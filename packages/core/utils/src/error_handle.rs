pub trait ErrorUtils {
    fn print_error(&self);
}

impl ErrorUtils for Box<dyn std::error::Error> {
    fn print_error(&self) {
        log::error!("Error: {}", self);
    }
}