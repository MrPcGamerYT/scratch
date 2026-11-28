"""Clean-launch usable-Home certification on the native engine (no APK install).

Boots the original Android initrd + system through native VirtIO block, drives a
Home launch over the private COM2 bridge (CHECK_HOME), and certifies usable Home:
launcher resumed, non-blank rendered scanout, clean crash capture, no definitive
ANR. Measures launch-to-Home against the ~100s product target (no APK overhead).
"""
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

HOME_PACKAGES = ('com.android.launcher3', 'org.lineageos.trebuchet', 'com.bliss.launcher', 'com.farmerbb.taskbar')
TARGET_MS = 100000


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(4194304), b''):
            h.update(chunk)
    return h.hexdigest()


def section(serial, begin, end):
    if begin not in serial:
        return ''
    rest = serial.split(begin, 1)[1]
    return rest.split(end, 1)[0] if end in rest else rest


def home_evidence(serial):
    selected = re.findall(r'(?m)^NEXUS_HOME_SELECTED ([A-Za-z0-9_.]+/[A-Za-z0-9_.$]+)\s*$', serial)
    selected = [v for v in selected if v.split('/', 1)[0] not in {'android', 'com.android.provision'}]
    component = selected[-1] if selected else None
    activity = section(serial, 'NEXUS_HOME_ACTIVITY_STATE_BEGIN', 'NEXUS_HOME_ACTIVITY_STATE_END')
    resumed = bool(component and any(component in line and 'ResumedActivity:' in line for line in activity.splitlines()))
    frames = section(serial, 'NEXUS_HOME_FRAME_STATS_BEGIN', 'NEXUS_HOME_FRAME_STATS_END')
    frame_count = re.search(r'Total frames rendered:\s*(\d+)', frames)
    return {
        'selected_component': component,
        'home_activity_resumed': resumed,
        'window_visible': 'NEXUS_HOME_WINDOW_VISIBLE' in serial,
        'crash_capture_complete': 'NEXUS_HOME_CRASH_LOG_BEGIN' in serial and 'NEXUS_HOME_CRASH_LOG_END' in serial,
        'frame_duration_abort_observed': 'Impossible totalDuration' in serial,
        'captured_native_crash': bool(re.search(r'Fatal signal|Abort message:|>>> .* <<<', section(serial, 'NEXUS_HOME_CRASH_LOG_BEGIN', 'NEXUS_HOME_CRASH_LOG_END'))),
        'launcher_process_ready': 'NEXUS_HOME_PROCESS_READY 1' in serial,
        'launcher_reported_frames': int(frame_count.group(1)) if frame_count else None,
    }


def anr_definitive(serial):
    anr = section(serial, 'NEXUS_HOME_ANR_BEGIN', 'NEXUS_HOME_ANR_END')
    return [line for line in anr.splitlines() if re.search(r'ANR in|Not responding', line, re.I) and 'No ANR' not in line]


def main():
    root = Path('native-apk-artifact')
    manifest = json.loads((root / 'SHA256.json').read_text())
    expected = {'android-system.zlag', 'kernel', 'initrd.img', 'manifest.json'}
    assert expected <= set(manifest), 'Missing native Android asset: ' + repr(sorted(expected - set(manifest)))
    for name in expected:
        info = manifest[name]
        path = root / name
        assert path.is_file() and not path.is_symlink() and path.stat().st_size == info['bytes'], name
        assert digest(path) == info['sha256'], name
    exe = str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    log = Path('native-home-serial.log')
    cmd = ('earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 androidboot.console=ttyS0 '
           'ignore_loglevel nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1 '
           'clocksource=tsc tsc=reliable root=/dev/ram0 ROOT=/dev/vda SRC=/android HWACCEL=0 '
           'virtio_mmio.device=4K@0xd0000000:5 rdinit=/init')
    p = subprocess.run([exe, 'probe-android-home-create', str(root / 'kernel'), str(log), cmd,
                        str(root / 'initrd.img'), str(root / 'android-system.zlag'), 'native-home.cow',
                        '--ram-mib=2048', '--renderer=software', '--framebuffer=native-home-software.ppm',
                        '--bridge-home-check'], capture_output=True, text=True, timeout=500)
    serial = log.read_text(errors='replace') if log.exists() else ''
    timing = Path(str(log) + '.timing.json')
    metrics = json.loads(timing.read_text()) if timing.exists() else {}
    home = home_evidence(serial)
    definitive = anr_definitive(serial)
    result = {
        'schema': 1, 'profile': 'clean-launch usable-Home certification, no APK install',
        'android_home_verified': False, 'target_met': False, 'android_home_target_ms': TARGET_MS,
        'engine_exit': p.returncode, 'stderr': p.stderr, 'metrics': metrics,
        'bridge_home_verified': 'NATIVE_BRIDGE_HOME_VERIFIED' in p.stdout,
        'home_activity_evidence': home,
        'anr_definitive_lines': definitive,
        'anr_free': not definitive,
        'base_sha256_before': manifest['android-system.zlag']['sha256'],
        'base_sha256_after': digest(root / 'android-system.zlag'),
        'serial_tail': serial[-12000:],
    }
    Path('native-home-checkpoint.json').write_text(json.dumps(result, indent=2))
    # Compact, downloadable gate summary.
    compact = {
        'engine_exit': result['engine_exit'], 'bridge_home_verified': result['bridge_home_verified'],
        'home_selected': home['selected_component'], 'home_activity_resumed': home['home_activity_resumed'],
        'window_visible': home['window_visible'], 'crash_capture_complete': home['crash_capture_complete'],
        'frame_duration_abort_observed': home['frame_duration_abort_observed'],
        'captured_native_crash': home['captured_native_crash'],
        'launcher_process_ready': home['launcher_process_ready'],
        'launcher_reported_frames': home['launcher_reported_frames'],
        'anr_free': result['anr_free'], 'anr_definitive_lines': definitive,
        'renderer': metrics.get('renderer'), 'android_home_ready_ms': metrics.get('android_home_ready_ms'),
        'total_boot_verified': metrics.get('total_boot_verified'),
    }
    Path('native-home-gate-summary.json').write_text(json.dumps(compact, indent=2))
    print('::notice::ANDROID_HOME_EVIDENCE=' + json.dumps(home), flush=True)
    print('::notice::ANDROID_HOME_READY_MS=' + str(metrics.get('android_home_ready_ms')), flush=True)
    for label, text in [('HOST', p.stdout + '\n' + p.stderr), ('SERIAL_TAIL', serial[-12000:])]:
        for at in range(0, len(text), 2800):
            print('::notice::NATIVE_HOME_' + label + '_' + str(at) + '=' + text[at:at + 2800].replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A'), flush=True)
    assert result['base_sha256_before'] == result['base_sha256_after'], 'Base disk changed'
    assert p.returncode == 0, 'Clean-launch Home gate failed: ' + p.stderr[-1000:]
    assert result['bridge_home_verified'], 'Engine did not report NATIVE_BRIDGE_HOME_VERIFIED'
    assert home['home_activity_resumed'] and home['crash_capture_complete'], home
    assert not home['frame_duration_abort_observed'] and not home['captured_native_crash'], home
    assert result['anr_free'], ('Definitive ANR present', definitive)
    capture = Path('native-home-software.ppm')
    scan = None
    if capture.exists() and capture.stat().st_size:
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from native_scanout_evidence import scanout_evidence
        scan = scanout_evidence(capture.read_bytes())
        result['scanout'] = {k: v for k, v in scan.items() if k != 'preview_png_base64'}
        Path('native-home-checkpoint.json').write_text(json.dumps(result, indent=2))
    assert scan and scan['nonuniform'], scan
    assert metrics.get('renderer') == 'software', 'Engine did not record renderer=software'
    home_ready_ms = metrics['android_home_ready_ms']
    assert home_ready_ms is not None, 'Engine did not record android_home_ready_ms'
    result['android_home_verified'] = True
    result['target_met'] = home_ready_ms <= TARGET_MS
    Path('native-home-checkpoint.json').write_text(json.dumps(result, indent=2))
    print('::notice::NEXUS_HOME_ACCEPTANCE=' + json.dumps({
        'clean_launch': True, 'home_resumed': home['home_activity_resumed'],
        'rendered': bool(scan and scan['nonuniform']), 'anr_free': result['anr_free'],
        'android_home_ready_ms': home_ready_ms, 'target_ms': TARGET_MS,
        'target_met': result['target_met'], 'android_home_verified': True,
        'boundary': 'Native engine clean-launch Android Home; not networking/audio/GPU/physical certification'}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        import traceback as _tb
        try:
            Path('native-home-gate-summary.json').write_text(json.dumps({'failure': 'pre-summary', 'error': str(error)[-1500:], 'traceback_tail': _tb.format_exc()[-1500:]}, indent=2))
        except Exception:
            pass
        print('::error::' + str(error)[-2000:].replace('%', '%25').replace('\n', '%0A'), flush=True)
        raise
