#![no_main]

use libfuzzer_sys::fuzz_target;

use atm0s_sdn_network::base::NeighboursControl;

fuzz_target!(|data: &[u8]| {
    let _ = NeighboursControl::try_from(data);
});
