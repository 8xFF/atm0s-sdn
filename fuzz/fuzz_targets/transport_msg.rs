#![no_main]

use atm0s_sdn_network::base::TransportMsg;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = TransportMsg::try_from(data);
});
