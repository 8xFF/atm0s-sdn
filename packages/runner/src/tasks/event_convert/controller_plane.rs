use sans_io_runtime::{bus::BusEvent, Owner, Task, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    controller_plane::{self, ControllerPlaneTask},
    data_plane, SdnChannel, SdnEvent, SdnExtOut, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, controller_plane::ChannelIn, controller_plane::EventIn> {
    if let TaskInput::Bus(_, SdnEvent::ControllerPlane(event)) = event {
        TaskInput::Bus((), event)
    } else {
        panic!("Invalid input type for ControllerPlane {:?}", event)
    }
}

///
///
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
///
pub fn convert_output<'a>(
    worker: u16,
    event: TaskOutput<controller_plane::ChannelIn, controller_plane::ChannelOut, controller_plane::EventOut>,
) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, ControllerPlaneTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::ControllerPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(channel)) => WorkerInnerOutput::Task(
            Owner::group(worker, ControllerPlaneTask::TYPE),
            TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::ControllerPlane(channel))),
        ),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            controller_plane::EventOut::Data(remote, data) => WorkerInnerOutput::Task(
                Owner::group(worker, ControllerPlaneTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::DataPlane(data_plane::ChannelIn::Worker(0)),
                    safe,
                    SdnEvent::DataPlane(data_plane::EventIn::Data(remote, data)),
                )),
            ),
        },
        _ => panic!("Invalid output type from ControllerPlane {:?}", event),
    }
}
