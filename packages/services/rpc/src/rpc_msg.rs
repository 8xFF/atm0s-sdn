use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_router::RouteRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum RpcError {
    Timeout,
    LocalQueueError,
    RemoteQueueError,
    DeserializeError,
    RuntimeError(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RpcMsgParam {
    Event(Vec<u8>),
    Request { req_id: u64, param: Vec<u8> },
    Answer { req_id: u64, param: Result<Vec<u8>, RpcError> },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RpcMsg {
    #[serde(skip)]
    pub from_node_id: NodeId,
    pub from_service_id: u8,
    pub cmd: String,
    pub param: RpcMsgParam,
}

impl RpcMsg {
    pub fn create_request<Req: Into<Vec<u8>>>(from_node_id: NodeId, from_service_id: u8, cmd: &str, req_id: u64, param: Req) -> RpcMsg {
        RpcMsg {
            from_node_id,
            from_service_id,
            cmd: cmd.to_string(),
            param: RpcMsgParam::Request { req_id, param: param.into() },
        }
    }

    pub fn create_event<Event: Into<Vec<u8>>>(from_node_id: NodeId, from_service_id: u8, cmd: &str, param: Event) -> RpcMsg {
        RpcMsg {
            from_node_id,
            from_service_id,
            cmd: cmd.to_string(),
            param: RpcMsgParam::Event(param.into()),
        }
    }

    pub fn create_answer<Res: Into<Vec<u8>>>(from_node_id: NodeId, from_service_id: u8, cmd: &str, req_id: u64, param: Result<Res, RpcError>) -> RpcMsg {
        RpcMsg {
            from_node_id,
            from_service_id,
            cmd: cmd.to_string(),
            param: RpcMsgParam::Answer {
                req_id,
                param: param.map(|p| p.into()),
            },
        }
    }

    pub fn req_id(&self) -> Option<u64> {
        match &self.param {
            RpcMsgParam::Request { req_id, param: _ } => Some(*req_id),
            RpcMsgParam::Answer { req_id, param: _ } => Some(*req_id),
            _ => None,
        }
    }

    pub fn is_request(&self) -> bool {
        matches!(&self.param, RpcMsgParam::Request { .. })
    }

    pub fn is_answer(&self) -> bool {
        matches!(&self.param, RpcMsgParam::Answer { .. })
    }

    pub fn is_event(&self) -> bool {
        matches!(&self.param, RpcMsgParam::Event { .. })
    }

    pub fn parse_event<E: for<'a> TryFrom<&'a [u8]>>(&self) -> Option<E> {
        if let RpcMsgParam::Event(e) = &self.param {
            E::try_from(e).ok()
        } else {
            None
        }
    }

    pub fn parse_request<Req: for<'a> TryFrom<&'a [u8]>>(&self) -> Option<(u64, Req)> {
        if let RpcMsgParam::Request { req_id, param } = &self.param {
            Req::try_from(param).ok().map(|req| (*req_id, req))
        } else {
            None
        }
    }

    pub fn parse_answer<Res: for<'a> TryFrom<&'a [u8]>>(&self) -> Option<(u64, Result<Res, RpcError>)> {
        if let RpcMsgParam::Answer { req_id, param } = &self.param {
            match param {
                Ok(buf) => {
                    let res = Res::try_from(buf).ok()?;
                    Some((*req_id, Ok(res)))
                }
                Err(e) => Some((*req_id, Err(e.clone()))),
            }
        } else {
            None
        }
    }

    pub fn answer<Res: Into<Vec<u8>>>(&self, from_node_id: NodeId, from_service_id: u8, param: Result<Res, RpcError>) -> RpcMsg {
        if let RpcMsgParam::Request { req_id, param: _ } = self.param {
            RpcMsg {
                cmd: self.cmd.clone(),
                from_node_id,
                from_service_id,
                param: RpcMsgParam::Answer {
                    req_id,
                    param: param.map(|r| r.into()),
                },
            }
        } else {
            panic!("Current msg is not a request")
        }
    }

    pub fn to_transport_msg(&self, to_service_id: u8, rule: RouteRule) -> TransportMsg {
        let mut header = MsgHeader::build(to_service_id, rule);
        header.from_node = Some(self.from_node_id);
        let buf: Vec<u8> = bincode::serialize(self).expect("Should serialize");
        TransportMsg::build_raw(header, &buf)
    }
}

impl TryFrom<&TransportMsg> for RpcMsg {
    type Error = ();

    fn try_from(value: &TransportMsg) -> Result<Self, Self::Error> {
        let from_node_id = value.header.from_node.ok_or(())?;
        let mut rpc = bincode::deserialize::<Self>(value.payload()).map_err(|_| ())?;
        rpc.from_node_id = from_node_id;
        Ok(rpc)
    }
}
