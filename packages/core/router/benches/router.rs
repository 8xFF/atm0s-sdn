use std::vec;

use atm0s_sdn_identity::ConnId;
use atm0s_sdn_router::core::{Metric, RegistrySync, Router, RouterSync};
use criterion::{criterion_group, criterion_main, Criterion};

criterion_group!(benches, benchmark_empty, benchmark_single, benchmark_full);
criterion_main!(benches);

fn benchmark_empty(c: &mut Criterion) {
    let mut group = c.benchmark_group("empty");
    group.throughput(criterion::Throughput::Elements(1));
    let router = Router::new(0);
    group.bench_function("next_node", |b| {
        b.iter(|| router.next(1, &[]));
    });

    group.bench_function("next_closest", |b| {
        b.iter(|| router.closest_node(rand::random(), &[]));
    });

    let router = Router::new(0);
    group.bench_function("next_service", |b| {
        b.iter(|| router.service_next(1, &[]));
    });
}

fn benchmark_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("single");
    group.throughput(criterion::Throughput::Elements(1));
    let mut router = Router::new(0);
    router.set_direct(ConnId::from_in(0, 0), 1, Metric::new(1, vec![1], 100000));
    group.bench_function("next_node", |b| {
        b.iter(|| router.next(1, &[]));
    });

    group.bench_function("next_closest", |b| {
        b.iter(|| router.closest_node(rand::random(), &[]));
    });

    let mut router = Router::new(0);
    router.set_direct(ConnId::from_in(0, 0), 1, Metric::new(1, vec![1], 100000));
    router.apply_sync(
        ConnId::from_in(0, 0),
        1,
        Metric::new(1, vec![1], 100000),
        RouterSync(RegistrySync(vec![(0, Metric::new(1, vec![], 100000))]), [None, None, None, None]),
    );
    group.bench_function("next_service", |b| {
        b.iter(|| router.service_next(1, &[]));
    });
}

fn benchmark_full(c: &mut Criterion) {
    let mut group = c.benchmark_group("full");
    group.throughput(criterion::Throughput::Elements(1));
    let mut router = Router::new(0);
    for n in 1..255 {
        router.set_direct(ConnId::from_in(0, n as u64), n, Metric::new(1, vec![n], 100000));
    }
    group.bench_function("next_node", |b| {
        b.iter(|| router.next(1, &[]));
    });

    group.bench_function("next_closest", |b| {
        b.iter(|| router.closest_node(rand::random(), &[]));
    });

    let mut router = Router::new(0);
    let mut services = vec![];
    for s in 0..255 {
        services.push((s, Metric::new(1, vec![1], 100000)));
    }
    router.set_direct(ConnId::from_in(0, 0), 1, Metric::new(1, vec![1], 100000));
    router.apply_sync(ConnId::from_in(0, 0), 1, Metric::new(1, vec![1], 100000), RouterSync(RegistrySync(services), [None, None, None, None]));
    group.bench_function("next_service", |b| {
        b.iter(|| router.service_next(1, &[]));
    });
}
