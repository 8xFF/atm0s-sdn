use std::time::Instant;

use atm0s_sdn::{
    convert_enum,
    services::visualization,
    tasks::{SdnChannel, SdnEvent, SdnExtIn, SdnExtOut, SdnOwner, SdnSpawnCfg, SdnWorkerInner},
};
use sans_io_runtime::{TaskSwitcher, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

use crate::sfu::{self, SfuOwner, SfuWorker};

#[repr(usize)]
enum TaskType {
    Sfu = 0,
    Sdn = 1,
}

impl TryFrom<usize> for TaskType {
    type Error = ();
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Sfu),
            1 => Ok(Self::Sdn),
            _ => Err(()),
        }
    }
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone)]
pub enum ExtIn {
    Sfu(sfu::ExtIn),
    Sdn(SdnExtIn<SC>),
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone)]
pub enum ExtOut {
    Sfu(sfu::ExtOut),
    Sdn(SdnExtOut<SE>),
}

#[derive(convert_enum::From, convert_enum::TryInto, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelId {
    Sfu(sfu::ChannelId),
    Sdn(SdnChannel),
}

#[derive(convert_enum::From, convert_enum::TryInto, Clone)]
pub enum Event {
    Sfu(sfu::SfuEvent),
    Sdn(SdnEvent<SC, SE, TC, TW>),
}

pub struct ICfg {
    pub sfu: sfu::ICfg,
    pub sdn: atm0s_sdn::tasks::SdnInnerCfg<SC, SE, TC, TW>,
}

#[derive(convert_enum::From, convert_enum::TryInto)]
pub enum SCfg {
    Sfu(sfu::SCfg),
    Sdn(SdnSpawnCfg),
}

pub type SC = visualization::Control;
pub type SE = visualization::Event;
pub type TC = ();
pub type TW = ();

#[derive(convert_enum::From, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerOwner {
    Sdn(SdnOwner),
    Sfu(SfuOwner),
}

pub struct RunnerWorker {
    worker: u16,
    sdn: SdnWorkerInner<SC, SE, TC, TW>,
    sfu: SfuWorker,
    switcher: TaskSwitcher,
}

impl WorkerInner<RunnerOwner, ExtIn, ExtOut, ChannelId, Event, ICfg, SCfg> for RunnerWorker {
    fn build(worker: u16, cfg: ICfg) -> Self {
        Self {
            worker,
            sdn: SdnWorkerInner::build(worker, cfg.sdn),
            sfu: SfuWorker::build(worker, cfg.sfu),
            switcher: TaskSwitcher::new(2),
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        self.sdn.tasks() + self.sfu.tasks()
    }

    fn spawn(&mut self, now: Instant, cfg: SCfg) {
        match cfg {
            SCfg::Sdn(cfg) => {
                self.sdn.spawn(now, cfg);
            }
            SCfg::Sfu(cfg) => {
                self.sfu.spawn(now, cfg);
            }
        }
    }

    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg>> {
        let s = &mut self.switcher;
        while let Some(current) = s.looper_current(now) {
            match current.try_into().ok()? {
                TaskType::Sdn => {
                    if let Some(out) = s.looper_process(self.sdn.on_tick(now)) {
                        return Some(self.process_sdn(out));
                    }
                }
                TaskType::Sfu => {
                    if let Some(out) = s.looper_process(self.sfu.on_tick(now)) {
                        return Some(self.process_sfu(out));
                    }
                }
            }
        }

        None
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<'a, RunnerOwner, ExtIn, ChannelId, Event>) -> Option<WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg>> {
        match event {
            WorkerInnerInput::Task(owner, event) => match owner {
                RunnerOwner::Sdn(owner) => {
                    let out = self.sdn.on_event(now, WorkerInnerInput::Task(owner, event.convert_into().expect("")))?;
                    self.switcher.queue_flag_task(TaskType::Sdn as usize);
                    Some(self.process_sdn(out))
                }
                RunnerOwner::Sfu(owner) => {
                    let out = self.sfu.on_event(now, WorkerInnerInput::Task(owner, event.convert_into().expect("")))?;
                    self.switcher.queue_flag_task(TaskType::Sfu as usize);
                    Some(self.process_sfu(out))
                }
            },
            WorkerInnerInput::Ext(ext) => match ext {
                ExtIn::Sdn(ext) => {
                    let out = self.sdn.on_event(now, WorkerInnerInput::Ext(ext))?;
                    self.switcher.queue_flag_task(TaskType::Sdn as usize);
                    Some(self.process_sdn(out))
                }
                ExtIn::Sfu(ext) => {
                    let out = self.sfu.on_event(now, WorkerInnerInput::Ext(ext))?;
                    self.switcher.queue_flag_task(TaskType::Sfu as usize);
                    Some(self.process_sfu(out))
                }
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg>> {
        let s = &mut self.switcher;
        while let Some(current) = s.queue_current() {
            match current.try_into().ok()? {
                TaskType::Sdn => {
                    if let Some(out) = s.queue_process(self.sdn.pop_output(now)) {
                        return Some(self.process_sdn(out));
                    }
                }
                TaskType::Sfu => {
                    if let Some(out) = s.queue_process(self.sfu.pop_output(now)) {
                        return Some(self.process_sfu(out));
                    }
                }
            }
        }

        None
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg>> {
        let s = &mut self.switcher;
        while let Some(current) = s.looper_current(now) {
            match current.try_into().ok()? {
                TaskType::Sdn => {
                    if let Some(out) = s.looper_process(self.sdn.shutdown(now)) {
                        return Some(self.process_sdn(out));
                    }
                }
                TaskType::Sfu => {
                    if let Some(out) = s.looper_process(self.sfu.shutdown(now)) {
                        return Some(self.process_sfu(out));
                    }
                }
            }
        }

        None
    }
}

impl RunnerWorker {
    fn process_sdn<'a>(
        &mut self,
        out: WorkerInnerOutput<'a, SdnOwner, SdnExtOut<SE>, SdnChannel, SdnEvent<SC, SE, TC, TW>, SdnSpawnCfg>,
    ) -> WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg> {
        self.switcher.queue_flag_task(TaskType::Sdn as usize);
        out.convert_into()
    }

    fn process_sfu<'a>(&mut self, out: WorkerInnerOutput<'a, SfuOwner, sfu::ExtOut, sfu::ChannelId, sfu::SfuEvent, sfu::SCfg>) -> WorkerInnerOutput<'a, RunnerOwner, ExtOut, ChannelId, Event, SCfg> {
        self.switcher.queue_flag_task(TaskType::Sfu as usize);
        out.convert_into()
    }
}
