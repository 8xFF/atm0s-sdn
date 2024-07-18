use std::{net::SocketAddr, time::Instant};

use atm0s_sdn::sans_io_runtime::{collections::DynamicDeque, return_if_none, return_if_some, Buffer, Task};
use str0m::{
    change::{DtlsCert, SdpOffer},
    ice::IceCreds,
    media::{KeyframeRequestKind, MediaKind, Mid},
    net::{Protocol, Receive},
    Candidate, Event as Str0mEvent, IceConnectionState, Input, Output, Rtc,
};

use super::TrackMedia;

pub struct WhipTaskBuildResult {
    pub task: WhipTask,
    pub ice_ufrag: String,
    pub sdp: String,
}

pub enum WhipInput {
    UdpPacket { from: SocketAddr, data: Buffer },
    KeyFrame(KeyframeRequestKind),
}

pub enum WhipOutput {
    UdpPacket { to: SocketAddr, data: Buffer },
    Started(String),
    Media(TrackMedia),
    Destroy,
}

pub struct WhipTask {
    backend_addr: SocketAddr,
    timeout: Option<Instant>,
    rtc: Rtc,
    audio_mid: Option<Mid>,
    video_mid: Option<Mid>,
    room: String,
    queue: DynamicDeque<WhipOutput, 6>,
}

impl WhipTask {
    pub fn build(backend_addr: SocketAddr, dtls_cert: DtlsCert, room: String, sdp: &str) -> Result<WhipTaskBuildResult, String> {
        let rtc_config = Rtc::builder().set_rtp_mode(true).set_ice_lite(true).set_dtls_cert(dtls_cert).set_local_ice_credentials(IceCreds::new());

        let ice_ufrag = rtc_config.local_ice_credentials().as_ref().expect("should have ice credentials").ufrag.clone();
        let mut rtc = rtc_config.build();
        rtc.direct_api().enable_twcc_feedback();

        rtc.add_local_candidate(Candidate::host(backend_addr, Protocol::Udp).expect("Should create candidate"));

        let offer = SdpOffer::from_sdp_string(&sdp).expect("Should parse offer");
        let answer = rtc.sdp_api().accept_offer(offer).expect("Should accept offer");
        let instance = Self {
            backend_addr,
            timeout: None,
            rtc,
            audio_mid: None,
            video_mid: None,
            room,
            queue: DynamicDeque::default(),
        };

        Ok(WhipTaskBuildResult {
            task: instance,
            ice_ufrag,
            sdp: answer.to_sdp_string(),
        })
    }

    fn pop_event_inner(&mut self, now: Instant) -> Option<WhipOutput> {
        while let Ok(out) = self.rtc.poll_output() {
            match out {
                Output::Timeout(timeout) => {
                    self.timeout = Some(timeout);
                    break;
                }
                Output::Transmit(send) => {
                    return WhipOutput::UdpPacket {
                        to: send.destination,
                        data: send.contents.to_vec().into(),
                    }
                    .into();
                }
                Output::Event(e) => match e {
                    Str0mEvent::Connected => {
                        log::info!("WhipServerTask connected");
                        return WhipOutput::Started(self.room.clone()).into();
                    }
                    Str0mEvent::MediaAdded(media) => {
                        log::info!("WhipServerTask media added: {:?}", media);
                        if media.kind == MediaKind::Audio {
                            self.audio_mid = Some(media.mid);
                        } else {
                            self.video_mid = Some(media.mid);
                        }
                    }
                    Str0mEvent::IceConnectionStateChange(state) => match state {
                        IceConnectionState::Disconnected => {
                            return WhipOutput::Destroy.into();
                        }
                        _ => {}
                    },
                    Str0mEvent::RtpPacket(rtp) => {
                        let media = TrackMedia::from_raw(rtp);
                        return Some(WhipOutput::Media(media));
                    }
                    _ => {}
                },
            }
        }

        None
    }
}

impl Task<WhipInput, WhipOutput> for WhipTask {
    /// Called on each tick of the task.
    fn on_tick(&mut self, now: Instant) {
        let timeout = return_if_none!(self.timeout);
        if now < timeout {
            return;
        }

        if let Err(e) = self.rtc.handle_input(Input::Timeout(now)) {
            log::error!("Error handling timeout: {}", e);
        }
        self.timeout = None;
    }

    /// Called when an input event is received for the task.
    fn on_event(&mut self, now: Instant, input: WhipInput) {
        match input {
            WhipInput::UdpPacket { from, data } => {
                if let Err(e) = self
                    .rtc
                    .handle_input(Input::Receive(now, Receive::new(Protocol::Udp, from, self.backend_addr, &data).expect("Should parse udp")))
                {
                    log::error!("Error handling udp: {}", e);
                }
            }
            WhipInput::KeyFrame(kind) => {
                if let Some(mid) = self.video_mid {
                    log::info!("Requesting keyframe for video mid: {:?}", mid);
                    self.rtc.direct_api().stream_rx_by_mid(mid, None).expect("Should has video mid").request_keyframe(kind);
                } else {
                    log::error!("No video mid for requesting keyframe");
                }
            }
        }
    }

    fn on_shutdown(&mut self, _now: Instant) {
        self.rtc.disconnect();
        self.queue.push_back(WhipOutput::Destroy.into())
    }

    /// Retrieves the next output event from the task.
    fn pop_output(&mut self, now: Instant) -> Option<WhipOutput> {
        return_if_some!(self.queue.pop_front());
        self.pop_event_inner(now)
    }
}
