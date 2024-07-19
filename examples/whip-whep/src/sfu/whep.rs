use std::{net::SocketAddr, time::Instant};

use atm0s_sdn::{
    base::Buffer,
    sans_io_runtime::{collections::DynamicDeque, return_if_none, return_if_some, Task},
};
use str0m::{
    change::{DtlsCert, SdpOffer},
    ice::IceCreds,
    media::{KeyframeRequestKind, MediaKind, Mid},
    net::{Protocol, Receive},
    Candidate, Event as Str0mEvent, IceConnectionState, Input, Output, Rtc,
};

use super::TrackMedia;

pub struct WhepTaskBuildResult {
    pub task: WhepTask,
    pub ice_ufrag: String,
    pub sdp: String,
}

pub enum WhepInput<'a> {
    UdpPacket { from: SocketAddr, data: Buffer },
    Media(&'a TrackMedia),
}

pub enum WhepOutput {
    UdpPacket { to: SocketAddr, data: Buffer },
    Started(String),
    RequestKey(KeyframeRequestKind),
    Destroy,
}

pub struct WhepTask {
    backend_addr: SocketAddr,
    timeout: Option<Instant>,
    rtc: Rtc,
    audio_mid: Option<Mid>,
    video_mid: Option<Mid>,
    room: String,
    queue: DynamicDeque<WhepOutput, 6>,
}

impl WhepTask {
    pub fn build(backend_addr: SocketAddr, dtls_cert: DtlsCert, room: String, sdp: &str) -> Result<WhepTaskBuildResult, String> {
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
            queue: Default::default(),
        };

        Ok(WhepTaskBuildResult {
            task: instance,
            ice_ufrag,
            sdp: answer.to_sdp_string(),
        })
    }

    fn pop_event_inner(&mut self, now: Instant) -> Option<WhepOutput> {
        while let Ok(out) = self.rtc.poll_output() {
            match out {
                Output::Timeout(timeout) => {
                    self.timeout = Some(timeout);
                    return None;
                }
                Output::Transmit(send) => {
                    return Some(WhepOutput::UdpPacket {
                        to: send.destination,
                        data: send.contents.to_vec().into(),
                    });
                }
                Output::Event(e) => match e {
                    Str0mEvent::Connected => {
                        log::info!("WhepServerTask connected");
                        return WhepOutput::Started(self.room.clone()).into();
                    }
                    Str0mEvent::MediaAdded(media) => {
                        log::info!("WhepServerTask media added: {:?}", media);
                        if media.kind == MediaKind::Audio {
                            self.audio_mid = Some(media.mid);
                        } else {
                            self.video_mid = Some(media.mid);
                        }
                    }
                    Str0mEvent::IceConnectionStateChange(state) => match state {
                        IceConnectionState::Disconnected => {
                            return WhepOutput::Destroy.into();
                        }
                        _ => {}
                    },
                    Str0mEvent::KeyframeRequest(req) => {
                        return Some(WhepOutput::RequestKey(req.kind));
                    }
                    _ => {}
                },
            }
        }

        None
    }
}

impl WhepTask {
    /// Called on each tick of the task.
    pub fn on_tick(&mut self, now: Instant) {
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
    pub fn on_event<'a>(&mut self, now: Instant, input: WhepInput<'a>) {
        match input {
            WhepInput::UdpPacket { from, data } => {
                if let Err(e) = self
                    .rtc
                    .handle_input(Input::Receive(now, Receive::new(Protocol::Udp, from, self.backend_addr, &data).expect("Should parse udp")))
                {
                    log::error!("Error handling udp: {}", e);
                }
                self.timeout = None;
            }
            WhepInput::Media(media) => {
                let (mid, nackable) = if media.pt == 111 {
                    (self.audio_mid, false)
                } else {
                    (self.video_mid, true)
                };

                if let Some(mid) = mid {
                    if let Some(stream) = self.rtc.direct_api().stream_tx_by_mid(mid, None) {
                        log::debug!("Write rtp for mid: {:?} {} {} {}", mid, media.seq_no, media.time, media.payload.len());
                        if let Err(e) = stream.write_rtp(
                            media.pt.into(),
                            media.seq_no.into(),
                            media.time,
                            Instant::now(),
                            media.marker,
                            Default::default(),
                            nackable,
                            media.payload.clone(),
                        ) {
                            log::error!("Error writing rtp: {}", e);
                        }
                        log::trace!("clear timeout with media");
                        self.timeout = None;
                    }
                } else {
                    log::error!("No mid for media {}", media.pt);
                }
            }
        }
    }

    /// Retrieves the next output event from the task.
    pub fn pop_output(&mut self, now: Instant) -> Option<WhepOutput> {
        return_if_some!(self.queue.pop_front());
        self.pop_event_inner(now)
    }

    pub fn on_shutdown(&mut self, _now: Instant) {
        self.rtc.disconnect();
        self.queue.push_back(WhepOutput::Destroy.into());
    }
}
