use crate::transport::{Transport, TransportConnector, TransportEvent};

pub struct MockTransportConnector {

}

pub struct MockTransport {

}

impl MockTransport {

}

#[async_trait::async_trait]
impl Transport for MockTransport {
    fn connector(&self) -> Box<dyn TransportConnector> {
        todo!()
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        todo!()
    }
}