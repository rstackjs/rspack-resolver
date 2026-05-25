use gungraun::{
  binary_benchmark, binary_benchmark_group, main, BinaryBenchmarkConfig, Callgrind, Command,
  FlamegraphConfig, FlamegraphKind,
};

#[binary_benchmark]
#[bench::resolve_dependencies("deps")]
#[bench::resolve_many_extensions("many_extensions")]
#[bench::resolve_dependencies_pnp("pnp")]
fn bench_resolver(scenario: &str) -> Command {
  Command::new(env!("CARGO_BIN_EXE_resolver_bench_driver"))
    .arg(scenario)
    .build()
}

binary_benchmark_group!(name = resolver_group, benchmarks = bench_resolver);

main!(
  config = BinaryBenchmarkConfig::default().env_clear(false).tool(
    Callgrind::default().flamegraph(
      FlamegraphConfig::default()
        .kind(FlamegraphKind::All)
        .normalize_differential(true),
    ),
  ),
  binary_benchmark_groups = resolver_group
);
