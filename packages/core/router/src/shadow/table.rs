use atm0s_sdn_identity::{NodeId, NodeIdType};

#[derive(Debug)]
pub struct ShadowTable<Remote> {
    layer: u8,
    dests: [Option<Remote>; 256],
}

impl<Remote: Copy> ShadowTable<Remote> {
    pub fn new(layer: u8) -> Self {
        Self { layer, dests: [None; 256] }
    }

    pub fn set(&mut self, index: u8, remote: Remote) {
        self.dests[index as usize] = Some(remote);
    }

    pub fn del(&mut self, index: u8) {
        self.dests[index as usize] = None;
    }

    pub fn next(&self, dest: NodeId) -> Option<Remote> {
        let index = dest.layer(self.layer);
        self.dests[index as usize]
    }

    /// Find the closest remote for the given key
    /// Returns the remote, the layer and the distance
    pub fn closest_for(&self, key_index: u8) -> Option<(Remote, u8, u8)> {
        let mut closest_distance: Option<(Remote, u8, u8)> = None;
        for i in 0..=255 {
            if let Some(remote) = self.dests[i as usize] {
                let distance = i ^ key_index;
                if closest_distance.is_none() || distance < closest_distance.expect("").2 {
                    closest_distance = Some((remote, i, distance));
                }
            }
        }
        closest_distance
    }
}
