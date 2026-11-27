"""Patch only the disposable, extracted ext4 copy. No guestfs/QEMU or user disks.
Install a fixed COM2 service, install verifier and bounded boot-diagnostic logger.
"""
import hashlib
import json
from pathlib import Path
import subprocess


def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda:f.read(4194304),b''): h.update(b)
    return h.hexdigest()


def main():
    image=Path('apk-system/system.img');before=sha(image)
    work=Path('apk-files');work.mkdir()
    rc='''service nexus-native /system/bin/nexus-native-agent
    class core
    user root
    group root system input log readproc
    seclabel u:r:su:s0
    disabled
    restart_period 5

service nexus-native-diag /system/bin/sh /system/bin/nexus-native-diag.sh
    class core
    user root
    group root system log readproc
    seclabel u:r:su:s0
    disabled
    oneshot

on post-fs-data
    start nexus-native
    start nexus-native-diag
'''
    check='''#!/system/bin/sh
exec >/dev/ttyS0 2>&1
# No set -e: ANR/home-repro/inventory checks are diagnostic and may fail
# intermittently. Only the APK install/uninstall steps are critical.
[ "$(getprop ro.build.version.sdk)" = 30 ] || { echo NEXUS_APK_CHECK_SDK_FAILED; exit 1; }
pm path org.nexus.smoke | grep '^package:' || { echo NEXUS_APK_CHECK_PRESENT_FAILED; exit 1; }
echo NEXUS_ANDROID_REAL_APK_PRESENT
pm uninstall org.nexus.smoke || { echo NEXUS_APK_CHECK_UNINSTALL_FAILED; exit 1; }
packages=$(pm list packages org.nexus.smoke)
if printf '%s\\n' "$packages" | grep -Fx 'package:org.nexus.smoke'; then echo NEXUS_APK_CHECK_STILL_PRESENT; exit 1; fi
echo NEXUS_ANDROID_REAL_APK_UNINSTALLED
echo NEXUS_SLIM_BEGIN
# Safe slimming: disable bloatware packages not needed for a launcher-only
# experience. Use pm disable (not uninstall) so the system can re-enable if
# needed. Only disable packages that are definitely not required for Home.
slim_count=0
for pkg in com.android.chrome com.google.android.gm com.google.android.apps.maps \
           com.google.android.apps.photos com.google.android.youtube \
           com.android.email com.android.mms com.android.soundrecorder \
           com.android.calculator2 com.android.deskclock com.android.spiral \
           com.android.browser com.google.android.apps.books \
           com.google.android.apps.movies com.google.android.apps.music \
           com.google.android.dialer com.google.android.apps.photos.lite \
           com.android.vending; do
    if pm list packages "$pkg" | grep -q "package:$pkg"; then
        pm disable --user 0 "$pkg" 2>/dev/null && slim_count=$((slim_count+1))
    fi
done
echo "NEXUS_SLIM_DISABLED $slim_count"
echo NEXUS_SLIM_END
echo NEXUS_ANR_BASELINE_BEGIN
if ! timeout 15 dumpsys activity lastanr; then echo NEXUS_ANR_BASELINE_UNAVAILABLE; fi
echo NEXUS_ANR_BASELINE_END
/system/bin/sh /system/bin/nexus-home-check.sh
echo NEXUS_HOME_REPRO_BEGIN
if ! timeout 20 am start -W -a android.intent.action.MAIN -c android.intent.category.HOME; then echo NEXUS_HOME_REPRO_UNAVAILABLE; fi
sleep 10
echo NEXUS_HOME_REPRO_END
echo NEXUS_ANR_AFTER_HOME_BEGIN
if ! timeout 10 dumpsys activity lastanr; then echo NEXUS_ANR_AFTER_HOME_UNAVAILABLE; fi
echo NEXUS_ANR_AFTER_HOME_END
echo NEXUS_ANR_EVENTS_BEGIN
if ! timeout 10 logcat -b events -d -t 100 -v brief am_anr:I am_crash:I am_activity_launch_time:I '*:S'; then echo NEXUS_ANR_EVENTS_UNAVAILABLE; fi
echo NEXUS_ANR_EVENTS_END
echo NEXUS_PACKAGE_INVENTORY_BEGIN
if ! timeout 10 pm list packages -f; then echo NEXUS_PACKAGE_INVENTORY_UNAVAILABLE; fi
echo NEXUS_PACKAGE_INVENTORY_END
exit 0
'''
    diag='''#!/system/bin/sh
exec >/dev/ttyS0 2>&1
echo NEXUS_NATIVE_DIAGNOSTICS_STARTED
for attempt in 1 2 3 4 5 6 7 8; do
    sleep 6
    echo NEXUS_NATIVE_GRAPHICS_LOGCAT
    logcat -b all -d -t 100 -v brief SurfaceFlinger:E AndroidRuntime:E DEBUG:E '*:S'
done
'''
    payloads={'nexus-native.rc':(rc,'/system/etc/init/nexus-native.rc',0o644),
              'nexus-apk-check.sh':(check,'/system/bin/nexus-apk-check.sh',0o755),
              'nexus-native-diag.sh':(diag,'/system/bin/nexus-native-diag.sh',0o755)}
    sources=[]
    for name,(text,target,mode) in payloads.items():
        source=work/name;source.write_text(text);sources.append((source,target,mode))
    sources.append((Path('guest-tools/nexus-guest/home-diagnostic.sh'),'/system/bin/nexus-home-check.sh',0o755))
    sources.append((Path('nexus-native-agent'),'/system/bin/nexus-native-agent',0o755))
    context=work/'file-context';context.write_bytes(b'u:object_r:system_file:s0\0')
    def debug(command,write=False):
        p=subprocess.run(['debugfs',*(['-w'] if write else []),'-R',command,str(image)],capture_output=True,text=True,timeout=30)
        print(command,p.stdout,p.stderr,flush=True)
        if p.returncode: raise RuntimeError('debugfs failed')
        return p.stdout
    for directory in ['/system/bin','/system/etc/init']:
        assert 'Type: directory' in debug('stat '+directory),'Unexpected Android root layout'
    manifest=[]
    for source,target,mode in sources:
        assert 'Inode:' not in debug('stat '+target),'Refusing existing target'
        debug(f'write {source} {target}',True)
        debug(f'set_inode_field {target} mode 0{0o100000|mode:o}',True)
        debug(f'ea_set -f {context} {target} security.selinux',True)
        verify=work/(source.name+'.readback');debug(f'dump {target} {verify}')
        assert verify.read_bytes()==source.read_bytes(),'Guest payload readback mismatch'
        label=work/(source.name+'.context');debug(f'ea_get -f {label} {target} security.selinux')
        assert label.read_bytes()==context.read_bytes(),'SELinux label readback mismatch'
        manifest.append({'target':target,'sha256':sha(source),'mode':oct(mode)})
    Path('native-apk-derived-manifest.json').write_text(json.dumps({'schema':1,'source_filesystem_sha256':before,'derived_filesystem_sha256':sha(image),'files':manifest,'selinux_policy_modified':False,'packages_removed':[],'android_apk_verified':False},indent=2))


if __name__=='__main__': main()
