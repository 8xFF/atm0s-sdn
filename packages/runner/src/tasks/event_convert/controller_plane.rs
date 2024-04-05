use std::fmt::Debug;

use atm0s_sdn_network::{ExtIn, ExtOut};
use sans_io_runtime::{bus::BusEvent, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    controller_plane::{self},
    data_plane, SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnOwner, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a, SC, SE, TC, TW>(
    event: TaskInput<'a, SdnExtIn<SC>, SdnChannel, SdnEvent<SC, SE, TC, TW>>,
) -> TaskInput<'a, ExtIn<SC>, controller_plane::ChannelIn, controller_plane::EventIn<SC, SE, TC>> {
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
pub fn convert_output<'a, SC, SE, TC: Debug, TW: Debug>(
    event: TaskOutput<ExtOut<SE>, controller_plane::ChannelIn, controller_plane::ChannelOut, controller_plane::EventOut<SE, TW>>,
) -> WorkerInnerOutput<'a, SdnOwner, SdnExtOut<SE>, SdnChannel, SdnEvent<SC, SE, TC, TW>, SdnSpawnCfg> {
    match event {
        TaskOutput::Ext(ext) => WorkerInnerOutput::Ext(true, ext),
        TaskOutput::Bus(BusEvent::ChannelSubscribe(channel)) => WorkerInnerOutput::Task(SdnOwner::Controller, TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::ControllerPlane(channel)))),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(channel)) => WorkerInnerOutput::Task(SdnOwner::Controller, TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::ControllerPlane(channel)))),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => {
            let channel = match event.dest() {
                atm0s_sdn_network::LogicEventDest::Broadcast => SdnChannel::DataPlane(data_plane::ChannelIn::Broadcast),
                atm0s_sdn_network::LogicEventDest::Any => SdnChannel::DataPlane(data_plane::ChannelIn::Worker(0)),
                atm0s_sdn_network::LogicEventDest::Worker(worker) => SdnChannel::DataPlane(data_plane::ChannelIn::Worker(worker)),
            };

            WorkerInnerOutput::Task(
                SdnOwner::Controller,
                TaskOutput::Bus(BusEvent::ChannelPublish(channel, safe, SdnEvent::DataPlane(data_plane::EventIn::FromController(event)))),
            )
        }
        _ => panic!("Invalid output type from ControllerPlane"),
    }
}
