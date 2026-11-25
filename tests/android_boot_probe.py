"""Evidence-producing Android live-boot probe. Never treats QEMU startup as Android success."""
import argparse
import json
import pathlib
import re
import socket
import subprocess
import time


def boot_recipe(tree):
    # ISOLINUX is a literal representation of the live recipe in this Bliss ISO;
    # its EFI GRUB config builds equivalent entries using shell-like functions.
    for config in sorted(tree.rglob('isolinux.cfg')):
        text = config.read_text(errors='replace')
        for stanza in re.split(r'(?m)^label\s+', text)[1:]:
            kernel_match = re.search(r'(?m)^\s*kernel\s+(\S+)', stanza)
            append = re.search(r'(?m)^\s*append\s+(.+)', stanza)
            if not kernel_match or not append:
                continue
            args = append[1].split()
            if any(token.startswith(('INSTALL=', 'AUTO_INSTALL=', 'DEBUG=')) for token in args):
                continue
            initrd_arg = next((token.split('=', 1)[1] for token in args if token.startswith('initrd=')), '')
            kernels = list(tree.rglob(pathlib.PurePosixPath(kernel_match[1]).name))
            images = list(tree.rglob(pathlib.PurePosixPath(initrd_arg).name)) if initrd_arg else []
            if kernels and images and not any('$' in token for token in args):
                return kernels[0], images[0], ' '.join(token for token in args if token != 'quiet' and not token.startswith('initrd=')), str(config)
    candidates = sorted(tree.rglob('grub.cfg')) + sorted(tree.rglob('menu.lst'))
    for config in candidates:
        lines = config.read_text(errors='replace').splitlines()
        for index, line in enumerate(lines):
            match = re.match(r'\s*(?:linux|linuxefi|kernel)\s+(\S+)\s*(.*)', line)
            if not match or 'install' in match[2].lower():
                continue
            kernel = list(tree.rglob(pathlib.PurePosixPath(match[1]).name))
            for following in lines[index + 1:index + 12]:
                initrd = re.match(r'\s*(?:initrd|initrdefi)\s+(\S+)', following)
                if initrd:
                    images = list(tree.rglob(pathlib.PurePosixPath(initrd[1]).name))
                    if kernel and images and '$' not in match[2]:
                        args = re.sub(r'\bquiet\b', '', match[2])
                        return kernel[0], images[0], args, str(config)
    raise RuntimeError('No supported literal live-boot kernel/initrd recipe found in ISO; refusing to guess')


# Diagnostic hypotheses, NOT production presets or measured optimizations.
PROFILES = {
    'baseline': ('max', ''),
    'no-avx': ('max,avx=off,avx2=off,f16c=off,fma=off', ''),
    'drm-gbm': ('max', 'HWC=drm GRALLOC=gbm'),
    'drm-gbm-no-avx': ('max,avx=off,avx2=off,f16c=off,fma=off', 'HWC=drm GRALLOC=gbm'),
}


def profile_recipe(profile, command_line):
    cpu, overrides = PROFILES[profile]
    keys = {option.split('=', 1)[0] for option in overrides.split()}
    preserved = [option for option in command_line.split() if option.split('=', 1)[0] not in keys]
    return cpu, ' '.join(preserved + overrides.split())


def assess_ui(properties, logcat):
    if re.search(r'Fatal signal[^\n]*(?:llvmpipe|surfaceflinger|setupwizard)', logcat, re.I):
        return False, 'Renderer/compositor/setup-wizard fatal signal captured; framework boot completion is not a usable desktop'
    required = ['[sys.boot_completed]: [1]', '[init.svc.surfaceflinger]: [running]', '[init.svc.bootanim]: [stopped]']
    if not all(item in properties for item in required):
        return False, 'Framework responded but compositor readiness/boot-animation stop was not established'
    return True, 'Service readiness only; screenshots still require visual inspection'


def capture_screen(port, filename):
    with socket.create_connection(('127.0.0.1', port), timeout=5) as sock:
        stream = sock.makefile('rwb', buffering=0)
        def execute(payload):
            stream.write((json.dumps(payload) + '\n').encode())
            while True:
                response = json.loads(stream.readline())
                if 'error' in response:
                    raise RuntimeError(str(response['error']))
                if 'return' in response:
                    return response
        stream.readline()
        execute({'execute': 'qmp_capabilities'})
        execute({'execute': 'screendump', 'arguments': {'filename': str(filename)}})


def run(args):
    output = pathlib.Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    result = {'status': 'failed', 'graphics': args.graphics,
              'scope': 'Linux TCG live ISO; not Windows WHPX, installed disk, or game validation'}
    process = None
    serial_file = None
    endpoint = None
    qmp_port = None
    started = time.monotonic()
    try:
        kernel, initrd, command_line, config = boot_recipe(pathlib.Path(args.tree))
        cpu, command_line = profile_recipe(args.profile, command_line)
        result['profile'] = args.profile
        result['guest_gpu_acceleration'] = False
        result.update(kernel=str(kernel), initrd=str(initrd), source_config=config, iso_kernel_args=command_line)
        # Reserve local ephemeral ports; CI runs each probe in its own isolated job.
        def free_port():
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0))
                return sock.getsockname()[1]
        qmp_port, adb_port = free_port(), free_port()
        command = ['qemu-system-x86_64', '-machine', 'q35', '-accel', 'tcg,thread=multi',
                   '-cpu', cpu, '-smp', '2', '-m', '3072', '-display', 'none',
                   '-kernel', str(kernel), '-initrd', str(initrd),
                   '-append', command_line + ' console=ttyS0,115200',
                   '-cdrom', str(pathlib.Path(args.iso).resolve()), '-boot', 'order=d',
                   '-netdev', f'user,id=net0,hostfwd=tcp:127.0.0.1:{adb_port}-:5555',
                   '-device', 'virtio-net-pci,netdev=net0',
                   '-qmp', f'tcp:127.0.0.1:{qmp_port},server=on,wait=off',
                   '-serial', 'stdio', '-no-reboot']
        command += ['-vga', 'std'] if args.graphics == 'std' else ['-vga', 'none', '-device', 'virtio-vga']
        result['command'] = command
        serial_file = (output / 'serial.log').open('wb')
        process = subprocess.Popen(command, stdout=serial_file, stderr=serial_file)
        endpoint = f'127.0.0.1:{adb_port}'
        deadline = time.monotonic() + args.timeout
        next_capture = 0
        captures = 0
        consecutive_complete = 0
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f'QEMU exited before verification: {process.returncode}')
            if time.monotonic() >= next_capture:
                captures += 1
                try:
                    capture_screen(qmp_port, output / f'screen-{captures:02}.ppm')
                except (OSError, ValueError, RuntimeError) as error:
                    (output / 'probe.log').open('a').write(str(error) + '\n')
                next_capture = time.monotonic() + 60
            try:
                subprocess.run(['adb', 'connect', endpoint], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=4)
                probe = subprocess.run(['adb', '-s', endpoint, 'shell', 'getprop', 'sys.boot_completed'], capture_output=True, text=True, timeout=4)
                consecutive_complete = consecutive_complete + 1 if probe.returncode == 0 and probe.stdout.strip() == '1' else 0
            except subprocess.TimeoutExpired:
                # An unavailable adbd is expected during boot, not a fatal probe error.
                consecutive_complete = 0
            if consecutive_complete >= 3:
                result['framework_boot_completed'] = True
                result['status'] = 'ui_unverified'
                # Allow a bounded transition out of boot animation before collecting evidence.
                for _ in range(9):
                    animation = subprocess.run(['adb', '-s', endpoint, 'shell', 'getprop', 'init.svc.bootanim'], capture_output=True, text=True, timeout=10)
                    if animation.returncode == 0 and animation.stdout.strip() == 'stopped':
                        break
                    time.sleep(10)
                result['visual_validation'] = 'Screenshots require inspection; boot_completed does not prove correct graphics'
                for name, command_args in [('getprop.txt', ['getprop']), ('logcat.txt', ['logcat', '-d', '-t', '2000'])]:
                    capture = subprocess.run(['adb', '-s', endpoint, 'shell', *command_args], capture_output=True, timeout=30)
                    (output / name).write_bytes(capture.stdout + capture.stderr)
                ready, reason = assess_ui((output / 'getprop.txt').read_text(errors='replace'), (output / 'logcat.txt').read_text(errors='replace'))
                result['ui_assessment'] = reason
                if ready:
                    result['status'] = 'framework_ready_visual_unverified'
                capture_screen(qmp_port, output / 'screen-final.ppm')
                break
            time.sleep(5)
        else:
            raise RuntimeError('Timed out without three confirmed Android boot-complete replies; ADB may be disabled/unavailable. Inspect serial and screenshots; this is not a diagnosis.')
    except Exception as error:
        result['status'] = 'failed'
        result['error'] = str(error)
    finally:
        # Capture late UI state and evidence on failure too, before stopping QEMU.
        if process is not None and process.poll() is None:
            if endpoint:
                for name, command_args in [('getprop.txt', ['getprop']), ('logcat.txt', ['logcat', '-d', '-t', '4000'])]:
                    try:
                        capture = subprocess.run(['adb', '-s', endpoint, 'shell', *command_args], capture_output=True, timeout=20)
                        if capture.returncode == 0:
                            (output / name).write_bytes(capture.stdout)
                    except (OSError, subprocess.TimeoutExpired):
                        pass
            if qmp_port:
                try:
                    capture_screen(qmp_port, output / 'screen-final.ppm')
                except (OSError, ValueError, RuntimeError):
                    pass
        result['elapsed_seconds'] = round(time.monotonic() - started, 1)
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        if serial_file:
            serial_file.close()
        (output / 'result.json').write_text(json.dumps(result, indent=2))
    return 0 if result['status'] == 'framework_ready_visual_unverified' else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--iso', required=True)
    parser.add_argument('--tree', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--graphics', choices=['std', 'virtio'], required=True)
    parser.add_argument('--profile', choices=list(PROFILES), default='baseline')
    parser.add_argument('--timeout', type=int, default=720)
    raise SystemExit(run(parser.parse_args()))
