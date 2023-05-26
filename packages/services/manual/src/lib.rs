pub static MANUAL_SERVICE_ID: u8 = 1;

mod behavior;
mod handler;
mod msg;

pub use behavior::{ManualBehavior, ManualBehaviorConf};
pub use handler::ManualHandler;
pub use msg::{ManualBehaviorEvent, ManualHandlerEvent, ManualMsg, ManualReq, ManualRes};

//TODO test this lib
