use std::time::Instant;

use sans_io_runtime::{bus::BusEvent, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    connection,
    events::ConnectionEvent,
    plane,
    transport_manager::{self, TransportManagerTask},
    transport_worker::TransportWorkerTask,
    SdnChannel, SdnEvent, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, transport_manager::ChannelIn, transport_manager::EventIn> {
    if let TaskInput::Bus(_, SdnEvent::TransportManager(event)) = event {
        TaskInput::Bus((), event)
    } else {
        panic!("Invalid input type for TransportManager")
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a>(
    worker: u16,
    now: Instant,
    event: TaskOutput<transport_manager::ChannelIn, transport_manager::ChannelOut, transport_manager::EventOut>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(_)) => WorkerInnerOutput::Task(
            Owner::group(worker, TransportManagerTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::TransportManager)),
        ),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(_)) => WorkerInnerOutput::Task(
            Owner::group(worker, TransportManagerTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::TransportManager)),
        ),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            transport_manager::EventOut::Transport(event) => WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::Plane, safe, SdnEvent::Plane(plane::EventIn::Transport(event)))),
            ),
            transport_manager::EventOut::Worker(event) => WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::TransportWorker(worker), safe, SdnEvent::TransportWorker(event))),
            ),
            transport_manager::EventOut::PassthroughConnectionData(conn, data) => WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::Connection(conn),
                    safe,
                    SdnEvent::Connection(connection::EventIn::Net(ConnectionEvent::Data(now, data))),
                )),
            ),
        },
        _ => panic!("Invalid output type from TransportManager"),
    }
}
