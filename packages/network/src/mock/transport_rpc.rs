use crate::transport::{RpcAnswer, TransportRpc};
use async_std::channel::{unbounded, Receiver, RecvError, Sender};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct MockRpcAnswer<Res> {
    output: Arc<Mutex<VecDeque<MockTransportRpcOutput<Res>>>>,
}

impl<Res> RpcAnswer<Res> for MockRpcAnswer<Res> {
    fn ok(&self, res: Res) {
        self.output.lock().push_back(MockTransportRpcOutput::Answer(Ok(res)));
    }

    fn error(&self, code: u32, message: &str) {
        self.output.lock().push_back(MockTransportRpcOutput::Answer(Err((code, message.to_string()))));
    }
}

pub enum MockTransportRpcInput<Req> {
    Req(u8, Req),
}

pub enum MockTransportRpcOutput<Res> {
    Answer(Result<Res, (u32, String)>),
}

pub struct MockTransportRpc<Req, Res> {
    rx: Receiver<MockTransportRpcInput<Req>>,
    output: Arc<Mutex<VecDeque<MockTransportRpcOutput<Res>>>>,
}

impl<Req, Res> MockTransportRpc<Req, Res> {
    pub fn new() -> (Self, Sender<MockTransportRpcInput<Req>>, Arc<Mutex<VecDeque<MockTransportRpcOutput<Res>>>>) {
        let (tx, rx) = unbounded();
        let output = Arc::new(Mutex::new(VecDeque::new()));
        (Self { rx, output: output.clone() }, tx, output)
    }
}

#[async_trait::async_trait]
impl<Req, Res> TransportRpc<Req, Res> for MockTransportRpc<Req, Res>
where
    Req: Send + Sync,
    Res: Send + Sync + 'static,
{
    async fn recv(&mut self) -> Result<(u8, Req, Box<dyn RpcAnswer<Res>>), ()> {
        match self.rx.recv().await {
            Ok(MockTransportRpcInput::Req(service_id, req)) => {
                let answer = Box::new(MockRpcAnswer { output: self.output.clone() });
                Ok((service_id, req, answer))
            }
            Err(_) => Err(()),
        }
    }
}
