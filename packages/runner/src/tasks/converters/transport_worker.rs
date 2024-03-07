use sans_io_runtime::{bus::BusEvent, NetOutgoing, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    connection, transport_manager,
    transport_worker::{self, TransportWorkerTask},
    SdnChannel, SdnEvent, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, transport_worker::ChannelIn, transport_worker::EventIn> {
    match event {
        TaskInput::Bus(_, SdnEvent::TransportWorker(event)) => TaskInput::Bus((), event),
        TaskInput::Net(event) => TaskInput::Net(event),
        _ => panic!("Invalid input type for transport_worker"),
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a>(
    worker: u16,
    event: TaskOutput<'a, transport_worker::ChannelIn, transport_worker::ChannelOut, transport_worker::EventOut>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(_)) => WorkerInnerOutput::Task(
            Owner::group(worker, TransportWorkerTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::TransportWorker(worker))),
        ),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(_)) => WorkerInnerOutput::Task(
            Owner::group(worker, TransportWorkerTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::TransportWorker(worker))),
        ),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            transport_worker::EventOut::Connection(conn, event) => WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::Connection(conn), safe, SdnEvent::Connection(connection::EventIn::Net(event)))),
            ),
            transport_worker::EventOut::UnhandleData(remote, data) => WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::TransportManager,
                    safe,
                    SdnEvent::TransportManager(transport_manager::EventIn::UnhandleNetData(remote, data)),
                )),
            ),
        },
        TaskOutput::Net(out) => match out {
            NetOutgoing::UdpListen { addr, reuse } => WorkerInnerOutput::Task(Owner::group(worker, TransportWorkerTask::TYPE), TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse })),
            NetOutgoing::UdpPacket { slot, to, data } => WorkerInnerOutput::Task(Owner::group(worker, TransportWorkerTask::TYPE), TaskOutput::Net(NetOutgoing::UdpPacket { slot, to, data })),
        },
        _ => panic!("Invalid output type from transport_worker"),
    }
}
