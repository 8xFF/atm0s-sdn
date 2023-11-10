use std::sync::{Arc, Mutex};

use rand::{distributions::Standard, prelude::Distribution};

pub trait Random<T> {
    fn random(&self) -> T;
}

pub struct RealRandom();

impl<T> Random<T> for RealRandom
where
    Standard: Distribution<T>,
{
    fn random(&self) -> T {
        rand::random()
    }
}

pub struct MockRandom<T>(Arc<Mutex<T>>);

impl<T> Default for MockRandom<T>
where
    T: Default,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> MockRandom<T>
where
    T: Clone + Copy + Default,
{
    pub fn fake(&self, value: T) {
        *self.0.lock().unwrap() = value;
    }
}

impl<T> Random<T> for MockRandom<T>
where
    T: Clone + Copy,
{
    fn random(&self) -> T {
        *self.0.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_random() {
        let real_random = RealRandom();
        let random_number: u32 = real_random.random();
        assert!(random_number > 0);
    }

    #[test]
    fn test_mock_random() {
        let mock_random = MockRandom::default();
        mock_random.fake(42);
        let random_number: u32 = mock_random.random();
        assert_eq!(random_number, 42);
    }
}
