use std::{net::SocketAddr, time::Duration};

use atm0s_sdn::tasks::*;
use sans_io_runtime::{backend::MioBackend, Controller};

fn main() {
    env_logger::init();
    let mut controler = SdnController::default();
    controler.add_worker::<_, SdnWorkerInner, MioBackend<128, 128>>(SdnInnerCfg { behaviours: Some(vec![]) }, None);

    controler.add_worker::<_, SdnWorkerInner, MioBackend<128, 128>>(SdnInnerCfg { behaviours: None }, None);

    loop {
        controler.process();
        std::thread::sleep(Duration::from_millis(10));
    }
}
