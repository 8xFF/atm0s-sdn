use super::msg::Seq;

#[derive(Default)]
pub struct SeqGenerator {
    current: u64,
}

impl SeqGenerator {
    pub fn next(&mut self) -> Seq {
        let ret = self.current;
        self.current += 1;
        Seq(ret)
    }
}
