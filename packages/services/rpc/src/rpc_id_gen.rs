#[derive(Default, Clone)]
pub struct RpcIdGenerate {
    seed: u64,
}

impl RpcIdGenerate {
    pub fn generate(&mut self) -> u64 {
        let value = self.seed;
        self.seed += 1;
        value
    }
}
