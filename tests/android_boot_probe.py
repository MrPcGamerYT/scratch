"""Evidence-producing Android live-boot probe. Never treats QEMU startup as Android success."""
import argparse
import json
import pathlib
import re
import socket
import subprocess
import time
from android_frame_metrics import summarize
from android_home_gate import check_home, install_boot_branding
from android_stability import capture as capture_stability


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
    'virgl': ('max,avx=off,avx2=off,f16c=off,fma=off', 'HWC=drm GRALLOC=gbm'),
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


def virgl_renderer_line(dump):
    return next((line.strip() for line in dump.splitlines() if re.search(r'\bGLES\s*:.*\bvirgl\b', line, re.I)), '')


def capture_adb_screen(endpoint, filename):
    capture = subprocess.run(['adb', '-s', endpoint, 'exec-out', 'screencap', '-p'], capture_output=True, timeout=45)
    if capture.returncode != 0 or not capture.stdout.startswith(b'\x89PNG\r\n\x1a\n') or b'IEND' not in capture.stdout[-12:]:
        raise RuntimeError('Guest GL screenshot failed or did not return a complete PNG')
    filename.write_bytes(capture.stdout)


def measure_settings(endpoint, output):
    def shell(*command):
        result = subprocess.run(['adb', '-s', endpoint, 'shell', *command], capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            raise RuntimeError(result.stderr or 'ADB workload command failed')
        return result.stdout
    launch = shell('am', 'start', '-W', '-a', 'android.settings.SETTINGS')
    (output / 'settings-launch.txt').write_text(launch)
    if 'Status: ok' not in launch or 'Error:' in launch:
        raise RuntimeError('Settings did not confirm launch; no frame-time claim made')
    time.sleep(3)
    shell('dumpsys', 'gfxinfo', 'com.android.settings', 'reset')
    started = time.monotonic()
    for _ in range(3):
        shell('input', 'swipe', '500', '650', '500', '220', '600')
        shell('input', 'swipe', '500', '220', '500', '650', '600')
    stats = shell('dumpsys', 'gfxinfo', 'com.android.settings', 'framestats')
    (output / 'settings-framestats.txt').write_text(stats)
    metrics = summarize(stats)
    metrics['workload'] = 'Android Settings, six alternating 600 ms vertical swipes'
    metrics['workload_wall_seconds'] = round(time.monotonic() - started, 2)
    (output / 'frame-metrics.json').write_text(json.dumps(metrics, indent=2))
    return metrics


def PathEscape(path):
    return str(pathlib.Path(path).resolve()).replace(',', ',,')


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
        if args.disk:
            kernel, initrd, command_line, config = None, None, 'root=/dev/ram0 SRC=/android', 'disk EFI bootloader (no injected kernel arguments)'
        else:
            if not args.tree or not args.iso:
                raise ValueError('Live boot requires --iso and --tree')
            kernel, initrd, command_line, config = boot_recipe(pathlib.Path(args.tree))
        cpu, command_line = profile_recipe(args.profile, command_line)
        result['profile'] = args.profile
        result['guest_gpu_acceleration'] = False
        result['requested_renderer'] = 'virgl-opengl' if args.profile == 'virgl' else '2d-scanout'
        result['physical_host_gpu_verified'] = False
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
        if args.disk:
            if not args.uefi_code or not args.uefi_vars:
                raise ValueError('Disk-only probe requires matching UEFI code and per-test writable variables')
            for flag in ['-kernel', '-initrd', '-append', '-cdrom', '-boot']:
                index = command.index(flag)
                del command[index:index + 2]
            command += ['-drive', f'file={PathEscape(args.disk)},format=qcow2',
                        '-drive', f'if=pflash,format=raw,readonly=on,file={PathEscape(args.uefi_code)}',
                        '-drive', f'if=pflash,format=raw,file={PathEscape(args.uefi_vars)}', '-boot', 'order=c']
            result['scope'] = 'Linux TCG installed-disk UEFI boot; not Windows or persistence certification'
        command += ['-vga', 'std'] if args.graphics == 'std' else ['-vga', 'none', '-device', 'virtio-vga']
        if args.profile == 'virgl':
            display_index = command.index('-display')
            command[display_index + 1] = 'sdl,gl=on'
            device_index = command.index('virtio-vga')
            command[device_index] = 'virtio-vga-gl'
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
                if probe.returncode:
                    result['adb_last_error'] = probe.stderr.strip()[-1000:]
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
                for name, command_args in [('getprop.txt', ['getprop']), ('logcat.txt', ['logcat', '-d', '-t', '2000']), ('surfaceflinger.txt', ['dumpsys', 'SurfaceFlinger'])]:
                    capture = subprocess.run(['adb', '-s', endpoint, 'shell', *command_args], capture_output=True, timeout=30)
                    (output / name).write_bytes(capture.stdout + capture.stderr)
                ready, reason = assess_ui((output / 'getprop.txt').read_text(errors='replace'), (output / 'logcat.txt').read_text(errors='replace'))
                result['ui_assessment'] = reason
                if args.profile == 'virgl':
                    result['guest_gles_renderer'] = virgl_renderer_line((output / 'surfaceflinger.txt').read_text(errors='replace'))
                if args.profile == 'virgl' and not result['guest_gles_renderer']:
                    ready = False
                    result['ui_assessment'] = 'Guest compositor did not report VirGL; refusing to call software fallback a renderer pass'
                if ready:
                    result['status'] = 'framework_ready_visual_unverified'
                    if args.measure_ui:
                        try:
                            result['ui_frame_metrics'] = measure_settings(endpoint, output)
                        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                            result['ui_frame_metrics'] = {'status': 'measurement_failed', 'error': str(error)}
                if getattr(args, 'require_home', False):
                    if not ready:
                        raise RuntimeError('Service readiness failed before Home gate')
                    try:
                        result['home_gate'] = check_home(endpoint, output, getattr(args, 'prepare_home', False))
                    finally:
                        # Capture even when Home/ANR gating fails; never clear the evidence.
                        try:
                            result['stability_snapshot'] = capture_stability(endpoint, output / 'stability')
                        except (OSError, RuntimeError) as diagnostic_error:
                            result['stability_snapshot'] = {'status': 'inconclusive', 'error': str(diagnostic_error)}
                    if result['stability_snapshot']['status'] != 'no_anr_in_capture':
                        raise RuntimeError('Post-Home ANR evidence failed or is incomplete; inspect stability diagnostics')
                    if getattr(args, 'boot_animation', ''):
                        result['boot_branding'] = install_boot_branding(endpoint, args.boot_animation)
                if args.profile == 'virgl':
                    capture_adb_screen(endpoint, output / 'screen-final.png')
                else:
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
        if process is not None and process.poll() is None and getattr(args, 'require_home', False) and result.get('home_gate', {}).get('resumed_home'):
            try:
                subprocess.run(['adb', '-s', endpoint, 'shell', 'svc', 'power', 'shutdown'], capture_output=True, timeout=15)
                process.wait(timeout=120)
                if process.returncode != 0:
                    raise RuntimeError('Engine exited abnormally during shutdown')
                result['clean_shutdown'] = True
            except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                result['status'] = 'failed'
                result['error'] = 'Clean template shutdown not established: ' + str(error)
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
    parser.add_argument('--iso', default='')
    parser.add_argument('--tree', default='')
    parser.add_argument('--output', required=True)
    parser.add_argument('--graphics', choices=['std', 'virtio'], required=True)
    parser.add_argument('--measure-ui', action='store_true')
    parser.add_argument('--require-home', action='store_true')
    parser.add_argument('--boot-animation', default='')
    parser.add_argument('--prepare-home', action='store_true', help='Only for new clean CI template overlays, never personal instances')
    parser.add_argument('--disk', default='')
    parser.add_argument('--uefi-code', default='')
    parser.add_argument('--uefi-vars', default='')
    parser.add_argument('--profile', choices=list(PROFILES), default='baseline')
    parser.add_argument('--timeout', type=int, default=720)
    raise SystemExit(run(parser.parse_args()))
