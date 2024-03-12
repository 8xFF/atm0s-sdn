#[derive(Debug)]
pub struct ShadowTable<Remote> {
    dests: [Option<Remote>; 256],
}

impl<Remote: Copy> ShadowTable<Remote> {
    pub fn new() -> Self {
        Self { dests: [None; 256] }
    }

    pub fn set(&mut self, index: u8, remote: Remote) {
        self.dests[index as usize] = Some(remote);
    }

    pub fn del(&mut self, index: u8) {
        self.dests[index as usize] = None;
    }
}
