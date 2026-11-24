//! Launch-planner microbenchmark, NOT a VM/game performance benchmark.
use nexus_core::runtime::{plan, Config};
use std::{hint::black_box, time::Instant};
fn main() {
    let config = Config { disk: "C:/Nexus/android.qcow2", firmware_dir: "C:/Nexus/qemu/share/qemu", uefi_code: "code.fd", uefi_vars: "vars.fd", memory_mb: 4096, cpus: 4, adb_port: 5555, flags: 5 };
    for _ in 0..1000 { black_box(plan(black_box(&config)).unwrap()); }
    let iterations = 100_000u32;
    let start = Instant::now();
    for _ in 0..iterations { black_box(plan(black_box(&config)).unwrap()); }
    let elapsed = start.elapsed();
    println!("Rust VM launch-planner microbenchmark: {} iterations, {:.1} ns/plan", iterations, elapsed.as_nanos() as f64 / iterations as f64);
    println!("Measures configuration construction only. Does not measure game FPS, input latency or QEMU speed. No comparison baseline.");
}
