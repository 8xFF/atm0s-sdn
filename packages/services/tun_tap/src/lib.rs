pub static TUNTAP_SERVICE_ID: u8 = 2;

mod behavior;
mod handler;
mod msg;

pub use behavior::TunTapBehavior;
pub use handler::TunTapHandler;
pub use msg::{TunTapBehaviorEvent, TunTapHandlerEvent, TunTapReq, TunTapRes};
