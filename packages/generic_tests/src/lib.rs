#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn ensure_vector_free() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let mut vec = Vec::new();
        let pre_stats = dhat::HeapStats::get();

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

        let post_stats = dhat::HeapStats::get();

        // assert heap usage is the same
        assert_eq!(pre_stats.curr_blocks, post_stats.curr_blocks);
    }

    #[test]
    fn ensure_hashmap_free() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let mut map = HashMap::new();

        let pre_stats = dhat::HeapStats::get();

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

        let post_stats = dhat::HeapStats::get();

        // assert heap usage is the same
        assert_eq!(pre_stats.curr_blocks, post_stats.curr_blocks);
    }

    #[async_std::test]
    async fn ensure_channel_free() {
        let _profiler = dhat::Profiler::builder().testing().build();
        let (tx, rx) = async_std::channel::bounded(1);

        let pre_stats = dhat::HeapStats::get();

        tx.try_send(1).expect("Should send");
        rx.recv().await.expect("Should recv");

        let post_stats = dhat::HeapStats::get();

        // assert heap usage is the same
        assert_eq!(pre_stats.curr_blocks, post_stats.curr_blocks);
    }
}
