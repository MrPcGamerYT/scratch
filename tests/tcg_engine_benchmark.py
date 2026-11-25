"""Actual bundled QEMU TCG integer workload, not Android/game/GPU performance.

Compare two isolated complete bundles; no arbitrary speed threshold. Validate
both guest computation and execution completion before publishing measurements.
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import tempfile
import time


def expected_result(count):
    # Exponentiate the LCG's affine transform mod 2**32, independently of x86.
    mask = 2**32 - 1
    a, b, x = 1664525, 1013904223, 12345
    mul, add = 1, 0
    while count:
        if count & 1:
            mul, add = a * mul & mask, (a * add + b) & mask
        a, b = a * a & mask, (a * b + b) & mask
        count >>= 1
    return (mul * x + add) & mask


def boot_sector(count):
    code = bytearray(b'\xfa\x31\xc0\x8e\xd8\xba\xe9\x00')
    def emit(text):
        for ch in text:
            code.extend((0xb0, ch, 0xee))
    emit(b'START\n')
    code += b'\x66\xb9' + count.to_bytes(4, 'little')
    code += b'\x66\xb8' + (12345).to_bytes(4, 'little')
    loop = len(code)
    code += b'\x66\x69\xc0' + (1664525).to_bytes(4, 'little')
    code += b'\x66\x05' + (1013904223).to_bytes(4, 'little')
    code += b'\x66\x49'
    code += bytes((0x75, (loop - len(code) - 2) & 255))
    code += b'\x66\x3d' + expected_result(count).to_bytes(4, 'little')
    # Wrong arithmetic jumps over success output/exit to the halt loop.
    success_size = len(b'OK\n') * 3 + 6
    code += bytes((0x75, success_size))
    emit(b'OK\n')
    code += b'\xba\xf4\x00\x30\xc0\xee'  # isa-debug-exit value zero -> exit 1
    code += b'\xf4\xeb\xfd'
    assert len(code) <= 510
    return code + bytes(510 - len(code)) + b'\x55\xaa' + bytes(1474560 - 512)


def measure(bundle, boot, repeats):
    bundle = pathlib.Path(bundle).resolve()
    exe = bundle / 'bin/qemu-system-x86_64.exe'
    firmware = bundle / 'share/qemu'
    samples = []
    for _ in range(repeats):
        args = [str(exe), '-L', str(firmware), '-accel', 'tcg,thread=single',
                '-machine', 'pc', '-cpu', 'max', '-smp', '1', '-m', '64',
                '-display', 'none', '-monitor', 'none', '-serial', 'none', '-nic', 'none',
                '-debugcon', 'stdio', '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04',
                '-drive', f'format=raw,if=floppy,file={boot}', '-boot', 'a', '-no-reboot']
        start = time.perf_counter()
        result = subprocess.run(args, capture_output=True, timeout=90)
        elapsed = time.perf_counter() - start
        if result.returncode != 1 or b'START\nOK\n' not in result.stdout.replace(b'\r', b''):
            raise RuntimeError(f'Guest computation failed: {result.returncode} {result.stdout!r} {result.stderr!r}')
        samples.append(round(elapsed, 6))
    return {'seconds_including_firmware_startup': samples,
            'median_seconds': statistics.median(samples)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', required=True)
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--output', default='tcg-engine-results.json')
    parser.add_argument('--iterations', type=int, default=50000000)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 1000000000:
        parser.error('iterations out of range')
    with tempfile.TemporaryDirectory() as directory:
        boot = pathlib.Path(directory) / 'integer.img'
        boot.write_bytes(boot_sector(args.iterations))
        # Alternate order across pairs to reduce systematic warm-up/order bias.
        results = {'baseline': [], 'candidate': []}
        for pair in range(3):
            order = ['baseline', 'candidate'] if pair % 2 == 0 else ['candidate', 'baseline']
            for name in order:
                results[name].extend(measure(getattr(args, name), boot, 1)['seconds_including_firmware_startup'])
    summary = {name: {'seconds': values, 'median_seconds': statistics.median(values)} for name, values in results.items()}
    summary['scope'] = 'Single-vCPU TCG integer LCG + firmware startup; NOT Android, WHPX, graphics, input latency or game FPS. Three samples each on one shared CI host.'
    summary['iterations'] = args.iterations
    summary['baseline_over_candidate_ratio'] = round(summary['baseline']['median_seconds'] / summary['candidate']['median_seconds'], 4)
    text = json.dumps(summary, indent=2)
    pathlib.Path(args.output).write_text(text + '\n')
    print(text)
    print('::notice title=TCG engine reference only::' + text.replace('\n', '%0A'))


if __name__ == '__main__':
    main()
