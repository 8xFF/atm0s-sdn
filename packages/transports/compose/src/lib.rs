#[macro_export]

macro_rules! compose_transport {
    ($name:ident, $($field:ident),*) => {
        use atm0s_sdn::compose_transport_desp::FutureExt as _;

        pub struct $name {
            $($field: std::sync::Arc<atm0s_sdn::compose_transport_desp::Mutex<Box<dyn atm0s_sdn::Transport>>>,)*
            connector: Box<dyn atm0s_sdn::TransportConnector>,
        }

        impl $name {
            pub fn new($($field: Box<dyn atm0s_sdn::Transport>),*) -> Self {
                $(let $field = Arc::new(atm0s_sdn::compose_transport_desp::Mutex::new($field));)*

                let transports = vec![$($field.clone()),*];

                Self {
                    connector: Box::new(ComposeConnector {
                        conn_ids: std::collections::HashMap::new(),
                        transports,
                    }),
                    $($field,)*
                }
            }
        }

        #[async_trait::async_trait]
        impl atm0s_sdn::Transport for $name {
            fn connector(&mut self) ->  &mut Box<dyn atm0s_sdn::TransportConnector>  {
                &mut self.connector
            }

            async fn recv(&mut self) -> Result<atm0s_sdn::TransportEvent, ()> {
                $(let mut $field = self.$field.lock();)*
                atm0s_sdn::compose_transport_desp::select! {
                    $(event = $field.recv().fuse() => event),*
                }
            }
        }

        pub struct ComposeConnector {
            conn_ids: std::collections::HashMap<atm0s_sdn::ConnId, usize>,
            transports: Vec<std::sync::Arc<atm0s_sdn::compose_transport_desp::Mutex<Box<dyn atm0s_sdn::Transport>>>>
        }

        impl atm0s_sdn::TransportConnector for ComposeConnector {
            fn create_pending_outgoing(&mut self, dest: atm0s_sdn::NodeAddr) -> Vec<atm0s_sdn::ConnId> {
                let mut conn_ids = Vec::new();
                for (i, transport) in self.transports.iter().enumerate() {
                    let mut transport = transport.lock();
                    let connector = transport.connector();
                    for conn_id in connector.create_pending_outgoing(dest.clone()) {
                        self.conn_ids.insert(conn_id, i);
                        conn_ids.push(conn_id);
                    }
                }
                conn_ids
            }

            fn continue_pending_outgoing(&mut self, conn_id: atm0s_sdn::ConnId) {
                match self.conn_ids.remove(&conn_id) {
                    Some(index) => {
                        let mut transport = self.transports[index].lock();
                        let connector = transport.connector();
                        connector.continue_pending_outgoing(conn_id);
                    }
                    _ => panic!("Invalid conn_id"),
                }
            }

            fn destroy_pending_outgoing(&mut self, conn_id: atm0s_sdn::ConnId) {
                match self.conn_ids.remove(&conn_id) {
                    Some(index) => {
                        let mut transport = self.transports[index].lock();
                        let connector = transport.connector();
                        connector.destroy_pending_outgoing(conn_id);
                    }
                    _ => panic!("Invalid conn_id"),
                }
            }
        }
    };
}
