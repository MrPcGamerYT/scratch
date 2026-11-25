# Nexus embedded engine

Nexus ships its own source-built, patched QEMU executable and dependencies in
`qemu/bin`; users do not install or run a separate system QEMU. Rust owns launch
policy/input work, while QEMU still implements CPU/device emulation and WHPX.
This is a QEMU derivative, not a new Rust-native hypervisor.

## Release LTO candidate

The release-lto-v1 build uses the pinned QEMU revision
`698104725efad4b29079d857dfdebbd804e34c99`, GCC `-O3`, generic CPU tuning, and
Meson link-time optimization. It retains WHPX, multi-threaded TCG, SDL, VirGL,
networking, audio and tools. `-march=native` is deliberately forbidden: building
on CI must not produce an engine that requires the runner's CPU instruction set.
`ENGINE-BUILD.txt` records compiler/configuration and patch hashes in the bundle.
The actual patched source and license notices remain included.

The existing Nexus SDL patch batches dirty-rectangle presents inside a refresh,
while keeping asynchronous updates immediate. Its executable VGA work-count test
checks reduced presentation calls; this is not proof of higher Android FPS.

`tests/tcg_engine_benchmark.py` compares the last baseline engine and the candidate
on the same Windows runner using alternating sample order, five samples each.
A boot sector executes 500 million 32-bit integer LCG steps and checks its final
result against independent affine exponentiation. Both correctness and guest
exit must pass. Wall time includes firmware/startup. Results and provenance are
uploaded as `Nexus-engine-reference-evidence`. No speed threshold is invented:
an optimization may be neutral or slower. This narrow workload cannot establish
Android responsiveness, GPU performance, WHPX stability or gaming superiority.

## Priorities and limits

1. WHPX is the hardware CPU path; TCG remains a slow compatibility path. An
   optimized TCG executable is not a replacement for hardware virtualization.
2. Android System UI ANRs observed in the installed-image renderer comparison
   remain unresolved. Compiler options are not an ANR fix.
3. VirGL transport exists but Windows guest rendering is not certified; DirectX
   and Vulkan guest renderers are not implemented.
4. No CPU-pinning, unsafe disk-cache, skipped-flush, forced native ISA, guessed
   FPS unlock or unsupported host-GPU claim is introduced by this build.
5. Engine certification still needs repeated Android workloads on real Windows
   virtualization/GPU hardware, including input, audio, network and persistence.

See `graphics-backends.md` for actual guest evidence, rather than treating a
successful compiler build or tiny boot sector as Android certification.

## Initial candidate result

Full source build/package run 45595712726 passed. The initial short 50-million-step
comparison produced candidate median 0.314163 s and baseline/candidate ratio
0.8934: **no measured speedup**. Three samples included firmware startup and were
too short/noisy for a broad conclusion. The benchmark was subsequently extended
to 500 million steps and five alternating samples per engine. The candidate
Player ZIP was 83,086,323 bytes versus the preceding 108,083,378-byte Player ZIP
(about 23% smaller); package size is not Android FPS. The engine is a candidate,
not a certified faster replacement. Neither run certifies Android ANR fixes.
