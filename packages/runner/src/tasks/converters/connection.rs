use sans_io_runtime::{bus::BusEvent, Owner, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    connection,
    events::{self, TransportWorkerEvent},
    plane, transport_manager, transport_worker, SdnChannel, SdnEvent, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, connection::ChannelIn, connection::EventIn> {
    match event {
        TaskInput::Bus(channel, SdnEvent::Connection(event)) => {
            if let SdnChannel::Connection(conn) = channel {
                TaskInput::Bus(conn, event)
            } else {
                panic!("Invalid channel type for Connection {:?}", channel);
            }
        }
        _ => panic!("Invalid input type for Connection {:?}", event),
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a>(
    worker: u16,
    owner: Owner,
    event: TaskOutput<'a, connection::ChannelIn, connection::ChannelOut, connection::EventOut>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(conn)) => WorkerInnerOutput::Task(owner, TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::Connection(conn)))),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(conn)) => WorkerInnerOutput::Task(owner, TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::Connection(conn)))),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            connection::EventOut::Disconnected(conn) => WorkerInnerOutput::Task(
                owner,
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::TransportManager,
                    safe,
                    SdnEvent::TransportManager(transport_manager::EventIn::Disconnected(conn)),
                )),
            ),
            connection::EventOut::Net(conn, data) => WorkerInnerOutput::Task(
                owner,
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::TransportWorker(transport_worker::ChannelIn::Worker(worker)),
                    safe,
                    SdnEvent::TransportWorker(TransportWorkerEvent::SendConn(conn, data)),
                )),
            ),
            connection::EventOut::ToBehaviorBus(conn, service, data) => WorkerInnerOutput::Task(
                owner,
                TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::Plane, safe, SdnEvent::Plane(plane::EventIn::FromHandlerBus(conn, service, data)))),
            ),
            connection::EventOut::ToHandleBus(from, to, service, data) => WorkerInnerOutput::Task(
                owner,
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::Connection(to),
                    safe,
                    SdnEvent::Connection(connection::EventIn::Bus(service, events::BusEvent::FromHandler(from, data))),
                )),
            ),
        },
        _ => panic!("Invalid output type from Connection"),
    }
}
