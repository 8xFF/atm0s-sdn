use std::{collections::hash_map::DefaultHasher, hash::Hasher};

// generate hash of string, it need to consistent with all nodes type, OS or device
pub fn hash_str(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(s.as_bytes());
    hasher.finish()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_hash_str() {
        assert_eq!(hash_str("hello"), 16350172494705860510);
        assert_eq!(hash_str("world"), 17970961829702799988);
    }
}
