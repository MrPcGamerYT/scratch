"""Regression and work-count benchmark for the Nexus SDL2 dirty-rect batching.

Runs a tiny real-mode VGA workload on the actual Windows QEMU build using the
SDL dummy driver. No Android, WHPX, GPU, FPS, or real-world speed claim.
"""
import json
import os
import pathlib
import queue
import re
import subprocess
import tempfile
import threading
import time


def make_vga_boot_sector():
    # Mode 13h, then update three widely separated VRAM regions every BIOS tick.
    code = bytearray(b'\xb8\x13\x00\xcd\x10\xb8\x00\xa0\x8e\xc0\xb3\x01')
    loop = len(code)
    code += b'\xfe\xc3'  # inc bl
    for address in (0, 25600, 51200):
        code += b'\x26\x88\x1e' + address.to_bytes(2, 'little')  # mov es:[addr], bl
    code += b'\xb4\x00\xcd\x1a\x89\xd6'  # BIOS ticks -> si
    wait = len(code)
    code += b'\xb4\x00\xcd\x1a\x39\xf2'  # wait for next tick
    code += bytes((0x74, (wait - (len(code) + 2)) & 0xff))  # je wait
    code += bytes((0xeb, (loop - (len(code) + 2)) & 0xff))  # jmp loop
    code += b'\0' * (510 - len(code)) + b'\x55\xaa'
    return code + b'\0' * (1474560 - len(code))


with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory)
    image, trace, screenshot = root / 'vga.img', root / 'trace.log', root / 'screen.ppm'
    image.write_bytes(make_vga_boot_sector())
    firmware = ['-L', os.environ['QEMU_DATA']] if os.environ.get('QEMU_DATA') else []
    env = os.environ.copy()
    env.update(SDL_VIDEODRIVER='dummy', SDL_RENDER_DRIVER='software', SDL_AUDIODRIVER='dummy')
    args = [os.environ.get('QEMU_SYSTEM', 'qemu-system-x86_64'), *firmware, '-accel', 'tcg', '-m', '64',
            '-display', 'sdl', '-vga', 'std', '-monitor', 'none', '-serial', 'none', '-nic', 'none',
            '-drive', f'format=raw,if=floppy,file={image}', '-boot', 'a', '-qmp', 'stdio',
            '-trace', f'enable=sdl2_2d_nexus_present,file={trace}']
    proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, bufsize=1, env=env)
    lines = queue.Queue()
    def reader():
        for line in proc.stdout:
            lines.put(line)
        lines.put(None)
    threading.Thread(target=reader, daemon=True).start()
    def receive():
        while True:
            line = lines.get(timeout=30)
            if line is None:
                raise RuntimeError('QEMU exited: ' + proc.stderr.read())
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    def command(name, arguments=None):
        request = {'execute': name, 'id': name}
        if arguments is not None:
            request['arguments'] = arguments
        proc.stdin.write(json.dumps(request) + '\n'); proc.stdin.flush()
        while True:
            reply = receive()
            if reply.get('id') == name:
                if 'error' in reply:
                    raise RuntimeError(str(reply['error']))
                return reply
    try:
        assert 'QMP' in receive()
        command('qmp_capabilities')
        time.sleep(8)
        command('screendump', {'filename': str(screenshot)})
        command('quit')
        proc.wait(timeout=15)
        if proc.returncode:
            raise RuntimeError('QEMU failed: ' + proc.stderr.read())
    finally:
        if proc.poll() is None:
            proc.kill(); proc.wait()
    ppm = screenshot.read_bytes()
    if not (ppm.startswith(b'P6\n320 200\n255\n') or ppm.startswith(b'P6\n640 400\n255\n')):
        raise RuntimeError(f'VGA workload did not reach graphics mode: {ppm[:60]!r}')
    text = trace.read_text(errors='replace')
    batches = [int(value) for value in re.findall(r'sdl2_2d_nexus_present.*rectangles=(\d+)', text)]
    saved = sum(max(0, count - 1) for count in batches)
    if not batches or not saved:
        raise RuntimeError(f'No coalesced VGA refresh observed. Trace: {text[-4000:]}')
    print(f'SDL dirty-rectangle workload: {sum(batches)} rectangle uploads, {len(batches)} presents; '
          f'{saved} presents avoided vs one per rectangle. Synthetic counts only, not FPS or CPU performance.')
