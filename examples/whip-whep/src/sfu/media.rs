use serde::{Deserialize, Serialize};
use str0m::rtp::RtpPacket;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMedia {
    pub seq_no: u64,
    pub pt: u8,
    pub time: u32,
    pub marker: bool,
    pub payload: Vec<u8>,
}

impl TrackMedia {
    pub fn from_raw(rtp: RtpPacket) -> Self {
        Self {
            seq_no: *rtp.seq_no,
            pt: *rtp.header.payload_type,
            time: rtp.header.timestamp,
            marker: rtp.header.marker,
            payload: rtp.payload,
        }
    }

    pub fn to_buffer(&self) -> Vec<u8> {
        bincode::serialize(self).expect("")
    }

    pub fn from_buffer(data: &[u8]) -> Self {
        bincode::deserialize(data).expect("")
    }
}
