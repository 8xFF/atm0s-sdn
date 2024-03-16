use crate::{controller_plane, data_plane};

impl<TW> From<controller_plane::BusOut<TW>> for data_plane::BusInput<TW> {
    fn from(value: controller_plane::BusOut<TW>) -> Self {
        match value {
            controller_plane::BusOut::Single(event) => match event {
                controller_plane::BusOutSingle::NeigboursControl(remote, control) => data_plane::BusInput::NeigboursControl(remote, control),
                controller_plane::BusOutSingle::NetDirect(feature, conn, buf) => data_plane::BusInput::NetDirect(feature, conn, buf),
                controller_plane::BusOutSingle::NetRoute(feature, rule, buf) => data_plane::BusInput::NetRoute(feature, rule, buf),
            },
            controller_plane::BusOut::Multiple(event) => match event {
                controller_plane::BusOutMultiple::Pin(conn, remote, ctx) => data_plane::BusInput::Pin(conn, remote, ctx),
                controller_plane::BusOutMultiple::UnPin(conn) => data_plane::BusInput::UnPin(conn),
                controller_plane::BusOutMultiple::ToFeatureWorkers(to) => data_plane::BusInput::FromFeatureController(to),
                controller_plane::BusOutMultiple::ToServiceWorkers(service, to) => data_plane::BusInput::FromServiceController(service, to),
            },
        }
    }
}

impl<'a, TC> From<data_plane::BusOutput<TC>> for controller_plane::BusIn<TC> {
    fn from(value: data_plane::BusOutput<TC>) -> Self {
        match value {
            data_plane::BusOutput::ForwardControlToController(service, to) => controller_plane::BusIn::ForwardControlFromWorker(service, to),
            data_plane::BusOutput::ForwardEventToController(service, to) => controller_plane::BusIn::ForwardEventFromWorker(service, to),
            data_plane::BusOutput::ForwardNetworkToController(feature, conn, msg) => controller_plane::BusIn::ForwardNetFromWorker(feature, conn, msg),
            data_plane::BusOutput::ForwardLocalToController(feature, buf) => controller_plane::BusIn::ForwardLocalFromWorker(feature, buf),
            data_plane::BusOutput::ToFeatureController(to) => controller_plane::BusIn::FromFeatureWorker(to),
            data_plane::BusOutput::ToServiceController(service, tc) => controller_plane::BusIn::FromServiceWorker(service, tc),
            data_plane::BusOutput::NeigboursControl(remote, control) => controller_plane::BusIn::NeigboursControl(remote, control),
        }
    }
}
