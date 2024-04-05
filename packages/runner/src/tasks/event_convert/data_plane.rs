use atm0s_sdn_network::ExtOut;
use sans_io_runtime::{bus::BusEvent, TaskInput, TaskOutput, WorkerInnerOutput};

use crate::tasks::{
    data_plane::{self, EventOut},
    SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnOwner, SdnSpawnCfg,
};

///
///
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
///
pub fn convert_input<'a, SC, SE, TC, TW>(event: TaskInput<'a, SdnExtIn<SC>, SdnChannel, SdnEvent<SC, SE, TC, TW>>) -> TaskInput<'a, (), data_plane::ChannelIn, data_plane::EventIn<SE, TW>> {
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
pub fn convert_output<'a, SC, SE, TC, TW>(
    event: TaskOutput<'a, ExtOut<SE>, data_plane::ChannelIn, data_plane::ChannelOut, data_plane::EventOut<SC, SE, TC>>,
) -> WorkerInnerOutput<'a, SdnOwner, SdnExtOut<SE>, SdnChannel, SdnEvent<SC, SE, TC, TW>, SdnSpawnCfg> {
    match event {
        TaskOutput::Ext(ext) => WorkerInnerOutput::Ext(true, ext),
        TaskOutput::Bus(BusEvent::ChannelSubscribe(channel)) => WorkerInnerOutput::Task(SdnOwner::Data, TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::DataPlane(channel)))),
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(channel)) => WorkerInnerOutput::Task(SdnOwner::Data, TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::DataPlane(channel)))),
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => WorkerInnerOutput::Task(
            SdnOwner::Data,
            match event {
                EventOut::ToController(event) => TaskOutput::Bus(BusEvent::ChannelPublish(SdnChannel::ControllerPlane(()), safe, SdnEvent::ControllerPlane(event))),
                EventOut::ToWorker(worker, event) => TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::DataPlane(data_plane::ChannelIn::Worker(worker)),
                    safe,
                    SdnEvent::DataPlane(data_plane::EventIn::FromWorker(event)),
                )),
            },
        ),
        TaskOutput::Net(out) => WorkerInnerOutput::Task(SdnOwner::Data, TaskOutput::Net(out)),
        TaskOutput::Destroy => WorkerInnerOutput::Task(SdnOwner::Data, TaskOutput::Destroy),
    }
}
