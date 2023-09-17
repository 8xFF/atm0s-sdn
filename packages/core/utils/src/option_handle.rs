pub trait OptionUtils {
    fn print_none(&self, msg: &str);
}

impl<T> OptionUtils for Option<T> {
    fn print_none(&self, msg: &str) {
        match self {
            Some(_) => {}
            None => {
                log::error!("None: {}", msg);
            }
        }
    }
}
