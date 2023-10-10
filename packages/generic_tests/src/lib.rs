#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn ensure_vector_free() {
        let mut vec = Vec::new();
        let info = allocation_counter::measure(|| {
            vec.push(1);
            vec.push(2);
            vec.push(3);
            vec.push(4);
            vec.push(5);

            vec.pop();
            vec.pop();
            vec.pop();
            vec.pop();
            vec.pop();

            vec.shrink_to_fit();
        });

        // assert heap usage is the same
        assert_eq!(info.count_current, 0);
    }

    #[test]
    fn ensure_hashmap_free() {
        let mut map = HashMap::new();
        let info = allocation_counter::measure(|| {
            map.insert(1, 1);
            map.insert(2, 2);
            map.insert(3, 3);
            map.insert(4, 4);
            map.insert(5, 5);

            map.remove(&1);
            map.remove(&2);
            map.remove(&3);
            map.remove(&4);
            map.remove(&5);

            map.shrink_to_fit();
        });

        // assert heap usage is the same
        assert_eq!(info.count_current, 0);
    }

    #[test]
    fn ensure_channel_free() {
        let (tx, rx) = async_std::channel::bounded(1);
        let info = allocation_counter::measure(|| {
            tx.try_send(1).expect("Should send");
            rx.try_recv().expect("Should recv");
        });

        // assert heap usage is the same
        assert_eq!(info.count_current, 0);
    }
}
