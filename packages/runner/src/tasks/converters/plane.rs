use sans_io_runtime::{bus::BusEvent, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    connection::{self, ConnectionTask},
    events,
    plane::{self, PlaneTask},
    transport_manager, SdnChannel, SdnEvent, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, plane::ChannelIn, plane::EventIn> {
    if let TaskInput::Bus(_, SdnEvent::Plane(event)) = event {
        TaskInput::Bus((), event)
    } else {
        panic!("Invalid input type for Plane")
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a>(worker: u16, event: TaskOutput<plane::ChannelIn, plane::ChannelOut, plane::EventOut>) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(_)) => WorkerInnerOutput::Task(Owner::group(worker, PlaneTask::TYPE), TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::Plane))),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(_)) => WorkerInnerOutput::Task(Owner::group(worker, PlaneTask::TYPE), TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::Plane))),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            plane::EventOut::ToHandlerBus(conn, service, data) => WorkerInnerOutput::Task(
                Owner::group(worker, ConnectionTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::Connection(conn),
                    safe,
                    SdnEvent::Connection(connection::EventIn::Bus(service, events::BusEvent::FromBehavior(data))),
                )),
            ),
            plane::EventOut::ConnectTo(addr) => WorkerInnerOutput::Task(
                Owner::group(worker, ConnectionTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::TransportManager,
                    safe,
                    SdnEvent::TransportManager(transport_manager::EventIn::ConnectTo(addr)),
                )),
            ),
            plane::EventOut::SpawnConnection(cfg) => WorkerInnerOutput::Spawn(SdnSpawnCfg { cfg }),
        },
        _ => panic!("Invalid output type from Plane"),
    }
}
