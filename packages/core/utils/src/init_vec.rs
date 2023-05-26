pub fn init_vec<T>(size: usize, builder: fn() -> T) -> Vec<T> {
    let mut vec = vec![];
    for _ in 0..size {
        vec.push(builder());
    }
    vec
}
