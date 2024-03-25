use atm0s_sdn_identity::NodeId;
use bincode::Options;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NeighboursConnectError {
    AlreadyConnected,
    InvalidSignature,
    InvalidData,
    InvalidState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NeighboursDisconnectReason {
    Shutdown,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeighboursControlCmds {
    ConnectRequest { from: NodeId, to: NodeId, session: u64 },
    ConnectResponse { session: u64, result: Result<(), NeighboursConnectError> },
    Ping { session: u64, seq: u64, sent_ms: u64 },
    Pong { session: u64, seq: u64, sent_ms: u64 },
    DisconnectRequest { session: u64, reason: NeighboursDisconnectReason },
    DisconnectResponse { session: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighboursControl {
    pub ts: u64,
    pub cmd: NeighboursControlCmds,
    pub signature: Vec<u8>,
}

impl TryFrom<&[u8]> for NeighboursControl {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.first() == Some(&255) {
            bincode::DefaultOptions::new().with_limit(1499).deserialize(&value[1..]).map_err(|_| ())
        } else {
            Err(())
        }
    }
}

impl TryInto<Vec<u8>> for &NeighboursControl {
    type Error = ();

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut buf = Vec::with_capacity(1500);
        buf.push(255);
        bincode::DefaultOptions::new().with_limit(1499).serialize_into(&mut buf, &self).map_err(|_| ())?;
        Ok(buf)
    }
}
