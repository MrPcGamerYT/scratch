"""Read-only developer diagnostics. Never clears ANRs, disables services or removes apps.
ADB is used by CI diagnostics only, not a production player dependency.
Reports may contain package names/logs: review before sharing.
"""
import json
from pathlib import Path
import re
import subprocess
import time

COMMANDS = {
    'lastanr': ('dumpsys', 'activity', 'lastanr'),
    'input': ('dumpsys', 'input'),
    'surfaceflinger': ('dumpsys', 'SurfaceFlinger'),
    'activity': ('dumpsys', 'activity', 'activities'),
    'cpu': ('dumpsys', 'cpuinfo'),
    'memory': ('dumpsys', 'meminfo'),
    'pressure': ('cat', '/proc/pressure/cpu', '/proc/pressure/memory', '/proc/pressure/io'),
    'disk': ('df', '-k', '/data', '/system'),
    'packages': ('pm', 'list', 'packages', '-f'),
    'disabled-packages': ('pm', 'list', 'packages', '-d'),
    'services': ('dumpsys', 'activity', 'services'),
    'events': ('logcat', '-b', 'events', '-d', '-t', '2000'),
    'logcat': ('logcat', '-b', 'main', '-b', 'system', '-b', 'crash', '-d', '-t', '2000'),
}


def assess(records):
    """Fail closed on missing evidence; no assertion of an ANR's root cause."""
    anr = records.get('lastanr', {})
    events = records.get('events', {})
    clean = anr.get('ok') and bool(re.search(r'\bno anr\b', anr.get('text', ''), re.I))
    text = events.get('text', '') if events.get('ok') else ''
    signals = sorted(set(re.findall(r'\b(?:am_anr|am_crash|am_low_memory|am_kill)\b', text)))
    anr_observed = (anr.get('ok') and not clean and bool(re.search(r'\bANR in\s+\S+', anr.get('text', '')))) or 'am_anr' in signals
    return {
        'status': 'anr_observed' if anr_observed else ('app_crash_observed' if 'am_crash' in signals else ('no_anr_in_capture' if clean and events.get('ok') else 'inconclusive')),
        'event_signals': signals,
        'root_cause': 'not established; correlate CPU, memory, IO, graphics and ANR traces',
        'full_app_stability_verified': False,
    }


def capture(endpoint, output, runner=subprocess.run):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    records = {}
    started = time.monotonic()
    for name, command in COMMANDS.items():
        try:
            p = runner(['adb', '-s', endpoint, 'shell', *command], capture_output=True, text=True, timeout=15)
            records[name] = {'ok': p.returncode == 0, 'text': p.stdout, 'error': p.stderr, 'exit': p.returncode}
        except (OSError, subprocess.TimeoutExpired) as error:
            records[name] = {'ok': False, 'text': '', 'error': str(error)}
        (output / (name + '.json')).write_text(json.dumps(records[name], indent=2), encoding='utf-8')
    summary = assess(records)
    summary.update({'collection_seconds': time.monotonic() - started,
                    'failed_commands': [name for name, value in records.items() if not value['ok']],
                    'scope': 'bounded read-only snapshot, not a soak test',
                    'guest_modified': False})
    (output / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    return summary


def observe(endpoint, output, seconds=30, interval=5, runner=subprocess.run, clock=time.monotonic, sleep=time.sleep):
    """Bounded post-Home observation, without clearing historical ANR/crash evidence.
    This checks logs over time, not app interaction or GPU performance.
    """
    if not 1 <= seconds <= 300 or not 1 <= interval <= seconds:
        raise ValueError('Observation must be 1..300 seconds with interval <= duration')
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    start = clock()
    samples = []
    while True:
        records = {}
        for name in ['lastanr', 'events']:
            try:
                p = runner(['adb', '-s', endpoint, 'shell', *COMMANDS[name]], capture_output=True, text=True, timeout=10)
                records[name] = {'ok': p.returncode == 0, 'text': p.stdout, 'error': p.stderr}
            except (OSError, subprocess.TimeoutExpired) as error:
                records[name] = {'ok': False, 'text': '', 'error': str(error)}
        assessment = assess(records)
        samples.append({'elapsed_seconds': clock() - start, 'assessment': assessment, 'records': records})
        if assessment['status'] != 'no_anr_in_capture' or clock() - start >= seconds:
            break
        sleep(min(interval, max(0, seconds - (clock() - start))))
    result = {'status': 'observation_passed' if all(s['assessment']['status'] == 'no_anr_in_capture' for s in samples) else 'observation_failed',
              'requested_seconds': seconds, 'elapsed_seconds': clock() - start, 'samples': samples,
              'app_interaction_verified': False, 'guest_modified': False}
    (output / 'observation.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    return result


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', required=True)
    parser.add_argument('--output', required=True, help='New private directory; refuses overwrite')
    args = parser.parse_args()
    report = capture(args.endpoint, args.output)
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['status'] == 'no_anr_in_capture' else 1)
