use atm0s_sdn_network::ExtOut;
use sans_io_runtime::{bus::BusEvent, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    data_plane::{self, DataPlaneTask},
    SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a, TC, TW>(event: TaskInput<'a, SdnExtIn, SdnChannel, SdnEvent<TC, TW>>) -> TaskInput<'a, (), data_plane::ChannelIn, data_plane::EventIn<TW>> {
    match event {
        TaskInput::Bus(SdnChannel::DataPlane(channel), SdnEvent::DataPlane(event)) => TaskInput::Bus(channel, event),
        TaskInput::Net(event) => TaskInput::Net(event),
        _ => panic!("Invalid input type for DataPlane"),
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a, TC, TW>(
    worker: u16,
    event: TaskOutput<'a, ExtOut, data_plane::ChannelIn, data_plane::ChannelOut, data_plane::EventOut<TC>>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg> {
    match event {
        TaskOutput::Ext(ext) => WorkerInnerOutput::Ext(true, ext),
        TaskOutput::Bus(BusEvent::ChannelSubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, DataPlaneTask::<(), ()>::TYPE),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::DataPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, DataPlaneTask::<(), ()>::TYPE),
            TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::DataPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => WorkerInnerOutput::Task(
            Owner::group(worker, DataPlaneTask::<(), ()>::TYPE),
            TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::ControllerPlane(()), safe, SdnEvent::ControllerPlane(event))),
        ),
        TaskOutput::Net(out) => WorkerInnerOutput::Task(Owner::group(worker, DataPlaneTask::<(), ()>::TYPE), TaskOutput::Net(out)),
        TaskOutput::Destroy => WorkerInnerOutput::Task(Owner::group(worker, DataPlaneTask::<(), ()>::TYPE), TaskOutput::Destroy),
    }
}
