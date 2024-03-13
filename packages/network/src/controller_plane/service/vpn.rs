pub const SERVICE_TYPE: u8 = 0;
pub const SERVICE_NAME: &str = "vpn";

use crate::controller_plane::Service;

#[derive(Debug, Default)]
pub struct VpnService;

impl Service for VpnService {
    fn service_type(&self) -> u8 {
        SERVICE_TYPE
    }

    fn discoverable(&self) -> bool {
        false
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }
}