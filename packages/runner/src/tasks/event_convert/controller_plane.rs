use std::fmt::Debug;

use atm0s_sdn_network::{ExtIn, ExtOut};
use sans_io_runtime::{bus::BusEvent, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    controller_plane::{self, ControllerPlaneTask},
    data_plane, SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a, TC, TW>(event: TaskInput<'a, SdnExtIn, SdnChannel, SdnEvent<TC, TW>>) -> TaskInput<'a, ExtIn, controller_plane::ChannelIn, controller_plane::EventIn<TC>> {
    match event {
        TaskInput::Bus(_, SdnEvent::ControllerPlane(event)) => TaskInput::Bus((), event),
        TaskInput::Ext(ext) => TaskInput::Ext(ext),
        _ => panic!("Invalid input type for ControllerPlane"),
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a, TC: Debug, TW: Debug>(
    worker: u16,
    event: TaskOutput<ExtOut, controller_plane::ChannelIn, controller_plane::ChannelOut, controller_plane::EventOut<TW>>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg> {
    match event {
        TaskOutput::Ext(ext) => WorkerInnerOutput::Ext(true, ext),
        TaskOutput::Bus(BusEvent::ChannelSubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, ControllerPlaneTask::<(), ()>::TYPE),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::ControllerPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, ControllerPlaneTask::<(), ()>::TYPE),
            TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::ControllerPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => {
            let channel = if event.is_broadcast() {
                SdnChannel::DataPlane(data_plane::ChannelIn::Broadcast)
            } else {
                SdnChannel::DataPlane(data_plane::ChannelIn::Worker(0))
            };

            WorkerInnerOutput::Task(
                Owner::group(worker, ControllerPlaneTask::<(), ()>::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(channel, safe, SdnEvent::DataPlane(event))),
            )
        }
        _ => panic!("Invalid output type from ControllerPlane"),
    }
}
