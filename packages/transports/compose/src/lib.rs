#[macro_export]

macro_rules! compose_transport {
    ($name:ident, $($field:ident: $ty:ty),*) => {
        atm0s_sdn::compose_transport_desp::paste! {
            use atm0s_sdn::Transport;
            use atm0s_sdn::compose_transport_desp::FutureExt as _;
            pub struct $name {
                connector: [<$name Connector>],
            }

            impl $name {
                pub fn new($($field: $ty,)*) -> Self {
                    Self {
                        connector: [<$name Connector>] {
                            conn_ids: std::collections::HashMap::new(),
                            $($field,)*
                        },
                    }
                }
            }

            #[async_trait::async_trait]
            impl atm0s_sdn::Transport for $name {
                fn connector(&mut self) ->  &mut dyn atm0s_sdn::TransportConnector  {
                    &mut self.connector
                }

                async fn recv(&mut self) -> Result<atm0s_sdn::TransportEvent, ()> {
                    atm0s_sdn::compose_transport_desp::select! {
                        $(event = self.connector.$field.recv().fuse() => event),*
                    }
                }
            }

            pub struct [<$name Connector>] {
                conn_ids: std::collections::HashMap<atm0s_sdn::ConnId, usize>,
                $($field: $ty,)*
            }

            impl atm0s_sdn::TransportConnector for [<$name Connector>] {
                fn create_pending_outgoing(&mut self, dest: atm0s_sdn::NodeAddr) -> Vec<atm0s_sdn::ConnId> {
                    let mut conn_ids = Vec::new();
                    let mut i = 0;

                    $(
                        for conn_id in self.$field.connector().create_pending_outgoing(dest.clone()) {
                            self.conn_ids.insert(conn_id, i);
                            conn_ids.push(conn_id);
                        }
                        i += 1;
                    )*
                    conn_ids
                }

                fn continue_pending_outgoing(&mut self, conn_id: atm0s_sdn::ConnId) {
                    if let Some(index) = self.conn_ids.remove(&conn_id) {
                        let mut i = 0;
                        $(
                            if i == index {
                                self.$field.connector().continue_pending_outgoing(conn_id);
                                return;
                            }
                            i += 1;
                        )*
                    }
                }

                fn destroy_pending_outgoing(&mut self, conn_id: atm0s_sdn::ConnId) {
                    if let Some(index) = self.conn_ids.remove(&conn_id) {
                        let mut i = 0;
                        $(
                            if i == index {
                                self.$field.connector().destroy_pending_outgoing(conn_id);
                                return;
                            }
                            i += 1;
                        )*
                    }
                }
            }
        }
    };
}
