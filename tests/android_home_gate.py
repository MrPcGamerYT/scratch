"""Explicit developer-only clean-template provisioning and strict Home evidence.
Not for personal disks. Does not bypass Google login, FRP, encryption or locks.
"""
import re
import subprocess
import time

HOME_PACKAGES = ('com.android.launcher3', 'org.lineageos.trebuchet', 'com.bliss.launcher', 'com.farmerbb.taskbar')


def choose_home(text):
    components = re.findall(r'(?m)^\s*([a-zA-Z][\w.]*/[\w.$]+)\s*$', text)
    return next((component for package in HOME_PACKAGES for component in components if component.split('/')[0] == package), '')


def foreground_is_home(text, component):
    package = re.escape(component.split('/')[0])
    return bool(re.search(r'(?:mResumedActivity|topResumedActivity|mCurrentFocus)[^\n]*\b' + package + r'/', text))


def adb(endpoint, *args):
    result = subprocess.run(['adb', '-s', endpoint, 'shell', *args], capture_output=True, text=True, timeout=45)
    if result.returncode:
        raise RuntimeError('Guest command failed: ' + ' '.join(args) + ': ' + (result.stdout + result.stderr)[-800:])
    return result.stdout


def check_home(endpoint, output, provision=False):
    sdk = adb(endpoint, 'getprop', 'ro.build.version.sdk').strip()
    if sdk != '30':
        raise RuntimeError('Android 11 candidate must report SDK 30, found ' + sdk)
    candidates = adb(endpoint, 'cmd', 'package', 'query-activities', '--brief', '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.HOME')
    (output / 'home-candidates.txt').write_text(candidates)
    component = choose_home(candidates)
    if not component:
        raise RuntimeError('No supported installed Home activity found; refusing guessed launcher')
    if provision:
        # Only used on a new, disposable CI template overlay with no accounts.
        adb(endpoint, 'locksettings', 'set-disabled', 'true')
        if adb(endpoint, 'locksettings', 'get-disabled').strip() != 'true':
            raise RuntimeError('Clean template swipe-lock disable did not persist')
        for namespace, key in [('global', 'device_provisioned'), ('secure', 'user_setup_complete')]:
            adb(endpoint, 'settings', 'put', namespace, key, '1')
        result = adb(endpoint, 'cmd', 'package', 'set-home-activity', '--user', '0', component)
        (output / 'set-home-result.txt').write_text(result)
    # Do not keep rewriting setup state on the second boot: persistence must pass.
    if adb(endpoint, 'settings', 'get', 'global', 'device_provisioned').strip() != '1' or adb(endpoint, 'settings', 'get', 'secure', 'user_setup_complete').strip() != '1':
        raise RuntimeError('Guest setup completion did not persist')
    adb(endpoint, 'wm', 'dismiss-keyguard')
    adb(endpoint, 'am', 'start', '-W', '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.HOME')
    focused = ''
    for _ in range(12):
        focused = adb(endpoint, 'dumpsys', 'activity', 'activities')
        if foreground_is_home(focused, component):
            break
        time.sleep(5)
    (output / 'home-activity.txt').write_text(focused)
    if not foreground_is_home(focused, component):
        raise RuntimeError('Home did not become the resumed activity; Setup Wizard or another window may be blocking it')
    errors = adb(endpoint, 'dumpsys', 'activity', 'lastanr')
    (output / 'lastanr.txt').write_text(errors)
    if not re.search(r'\bno anr\b', errors, re.I):
        raise RuntimeError('ANR-free state not established; inspect lastanr.txt')
    return {'sdk': sdk, 'home_component': component, 'resumed_home': True, 'lastanr': 'none', 'provisioned_this_boot': provision}


def install_boot_branding(endpoint, animation):
    import hashlib
    from pathlib import Path
    subprocess.run(['adb', '-s', endpoint, 'root'], capture_output=True, timeout=20)
    for _ in range(12):
        try:
            if adb(endpoint, 'id', '-u').strip() == '0':
                break
        except (RuntimeError, subprocess.TimeoutExpired):
            pass
        time.sleep(2)
    else:
        raise RuntimeError('Clean template does not allow adb root; refusing unsafe system-image modification')
    result = subprocess.run(['adb', '-s', endpoint, 'push', str(animation), '/data/local/bootanimation.zip'], capture_output=True, text=True, timeout=45)
    if result.returncode:
        raise RuntimeError('Nexus boot asset upload failed: ' + result.stderr)
    adb(endpoint, 'chmod', '0644', '/data/local/bootanimation.zip')
    adb(endpoint, 'restorecon', '/data/local/bootanimation.zip')
    adb(endpoint, 'rm', '-f', '/data/.nexus-provision', '/data/.nexus-provision-adb.pub', '/data/misc/adb/adb_keys')
    adb(endpoint, 'settings', 'put', 'global', 'device_name', 'Nexus Android 11')
    expected = hashlib.sha256(Path(animation).read_bytes()).hexdigest()
    actual = adb(endpoint, 'sha256sum', '/data/local/bootanimation.zip').split()[0]
    if actual != expected:
        raise RuntimeError('Guest boot asset checksum mismatch')
    return {'path': '/data/local/bootanimation.zip', 'sha256': actual, 'visible_animation_verified': False,
            'note': 'User-data boot override; original system payload and upstream notices unchanged. Visibility needs second-boot screenshot review.'}
