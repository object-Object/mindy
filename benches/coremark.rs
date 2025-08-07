use iai_callgrind::{
    BinaryBenchmarkConfig, Command, Sandbox, binary_benchmark, binary_benchmark_group, main,
};

#[binary_benchmark]
#[bench::coremark_10(
    args = ("mlogv32.msch", "coremark_10.bin"),
    config = BinaryBenchmarkConfig::default()
        .sandbox(
            Sandbox::new(true)
                .fixtures([
                    "benches/coremark_10.bin",
                    "schematics/mlogv32.msch",
                ])
        ),
)]
fn bench_coremark_10(schem: &str, bin: &str) -> Command {
    Command::new(env!("CARGO_BIN_EXE_mlogv32"))
        .args([schem, "--bin", bin, "--delta=6", "--no-tui"])
        .build()
}

binary_benchmark_group!(name = coremark; benchmarks = bench_coremark_10);
main!(binary_benchmark_groups = coremark);
