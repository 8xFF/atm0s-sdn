use std::collections::HashMap;
use bluesea_identity::PeerAddr;

pub enum RpcClientError {
    TransportError,
    ProcessError(String),
}

#[async_trait::async_trait]
pub trait RpcClient {
    async fn request<P, R>(&self, dest: &PeerAddr, params: P) -> Result<R, RpcClientError>;
}

pub trait RpcServer {

}

pub trait RpcService<Req, Res> {
    fn name<'a>(&self) -> &'a str;
    fn on_req(&mut self, req: Req) -> Result<Res, String>;
}

pub struct RpcModule {
    services: HashMap<String, Box<dyn >>
}