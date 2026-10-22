use criterion::{criterion_group, criterion_main, Criterion, Throughput, BatchSize};
use std::hint::black_box;
use rf16::{compile_bf, VM};

fn bench_nested_loops(c: &mut Criterion) {
    let program = compile_bf("+++++[>+++++[>+++++<-]<-]");
    c.bench_function("nested loops", |b| {
        b.iter(|| {
            let mut vm = VM::new(black_box(program.clone()));
            vm.run();
            black_box(vm.ram[0]);
        })
    });
}

fn bench_badapple(c: &mut Criterion) {
    let src = include_str!("../examples/badapple.b");
    let program = compile_bf(src); // compiled once

    let mut group = c.benchmark_group("badapple");
    group.sample_size(10); // 100 samples is too many for a 12ms benchmark
    group.throughput(Throughput::Elements({
        let mut vm = VM::new(program.clone());
        vm.run();
        vm.steps as u64
    }));

    group.bench_function("run", |b| {
        b.iter_batched(
            || VM::new(program.clone()), // setup: clone happens outside timing
            |mut vm| { vm.run(); black_box(vm.ram[0]); }, // only this is measured
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_nested_loops, bench_badapple);
criterion_main!(benches);