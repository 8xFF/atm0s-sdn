pub static KEY_VALUE_SERVICE_ID: u8 = 4;
pub type KeyId = u64;
pub type ValueType = Vec<u8>;

mod behavior;
mod handler;
pub mod logic;
pub mod msg;
