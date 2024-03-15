use atm0s_sdn_identity::NodeId;
use bincode::Options;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeighboursConnectError {
    AlreadyConnected,
    InvalidSignature,
    InvalidData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeighboursDisconnectReason {
    Shutdown,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeighboursDisconnectError {
    WrongSession,
    NotConnected,
    InvalidSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeighboursControl {
    ConnectRequest {
        from: NodeId,
        to: NodeId,
        session: u64,
        signature: Vec<u8>,
    },
    ConnectResponse {
        from: NodeId,
        to: NodeId,
        session: u64,
        signature: Vec<u8>,
        result: Result<(), NeighboursConnectError>,
    },
    Ping {
        session: u64,
        seq: u64,
        sent_us: u64,
    },
    Pong {
        session: u64,
        seq: u64,
        sent_us: u64,
    },
    DisconnectRequest {
        session: u64,
        signature: Vec<u8>,
        reason: NeighboursDisconnectReason,
    },
    DisconnectResponse {
        session: u64,
        signature: Vec<u8>,
        result: Result<(), NeighboursDisconnectError>,
    },
}

impl TryFrom<&[u8]> for NeighboursControl {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value[0] == 255 {
            bincode::DefaultOptions::new().with_limit(1500).deserialize(&value[1..]).map_err(|_| ())
        } else {
            Err(())
        }
    }
}

impl TryInto<Vec<u8>> for &NeighboursControl {
    type Error = ();

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        bincode::DefaultOptions::new().with_limit(1500).serialize(&self).map_err(|_| ())
    }
}
