"""Add a static agent and init service to a copied Android-x86 ramdisk.
Preserve all original newc records/metadata. Fail closed on unsupported formats.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import stat


def records(data):
    offset = 0
    while offset + 110 <= len(data):
        start = offset
        header = data[offset:offset+110]
        if header[:6] != b'070701':
            raise ValueError('Only plain newc CPIO is supported')
        fields = [int(header[6+i*8:14+i*8],16) for i in range(13)]
        size, namesize = fields[6], fields[11]
        name = data[offset+110:offset+110+namesize-1].decode()
        offset = (offset+110+namesize+3)&~3
        body = data[offset:offset+size]
        if len(body) != size:
            raise ValueError('Truncated CPIO')
        offset = (offset+size+3)&~3
        if name == 'TRAILER!!!':
            return
        yield name, fields, body
    raise ValueError('Missing CPIO trailer')


def record(name, fields, body):
    fields = list(fields); encoded = name.encode()+b'\0'
    fields[6], fields[11] = len(body),len(encoded)
    data = b'070701'+b''.join(f'{value:08x}'.encode() for value in fields)+encoded
    data += bytes((-len(data))%4)
    data += body
    return data+bytes((-len(data))%4)


def patch(data, agent):
    compressed = data.startswith(b'\x1f\x8b')
    raw = gzip.decompress(data) if compressed else data
    items = list(records(raw))
    names = {name.lstrip('./'):name for name,_,_ in items}
    if 'init.rc' not in names:
        raise ValueError('No root init.rc in ramdisk; refusing guessed service injection. Entries: '+ ', '.join(list(names)[:40]))
    if 'init.nexus.rc' in names or 'nexus-agent' in names:
        raise ValueError('Bridge already present; use a pristine source ramdisk')
    output = bytearray()
    for name, fields, body in items:
        if name == names['init.rc']:
            if not stat.S_ISREG(fields[1]):
                raise ValueError('Root init.rc is not a regular file; refusing symlink corruption')
            body += b'\n# Nexus private guest integration\nimport /init.nexus.rc\n'
        output += record(name,fields,body)
    init = b'''service nexus-guest /nexus-agent
    class core
    user root
    group root system input
    seclabel u:r:su:s0
    disabled
    restart_period 5

on post-fs-data
    start nexus-guest
'''
    fields = [0, stat.S_IFREG|0o644, 0,0,1,0,0,0,0,0,0,0,0]
    output += record('init.nexus.rc',fields,init)
    fields[1] = stat.S_IFREG|0o755
    output += record('nexus-agent',fields,agent)
    output += record('nexus-home-check.sh',fields,b"#!/system/bin/sh\nexec >/dev/console 2>&1\nset -x\n[ \"$(getprop ro.build.version.sdk)\" = 30 ] || exit 1\n[ \"$(settings get global device_provisioned)\" = 1 ] || exit 1\n[ \"$(settings get secure user_setup_complete)\" = 1 ] || exit 1\n[ ! -e /data/.nexus-provision ] || exit 1\n[ \"$(locksettings get-disabled)\" = true ] || exit 1\nwm dismiss-keyguard || exit 1\nam start -W -a android.intent.action.MAIN -c android.intent.category.HOME >/dev/null || exit 1\nsleep 5\ndumpsys activity activities | grep -E '(mResumedActivity|topResumedActivity).* (com.android.launcher3|org.lineageos.trebuchet|com.bliss.launcher|com.farmerbb.taskbar)/' >/dev/null || exit 1\nset +x\nanr=$(dumpsys activity lastanr)\nprintf '%s\\n' \"$anr\" | head -n 45\nif ! printf '%s\\n' \"$anr\" | grep -qi 'no anr'; then\n  logcat -d -v brief -t 250 ActivityManager:W AndroidRuntime:E '*:S'\n  exit 1\nfi\n")
    output += record('nexus-provision-home.sh',fields,b"#!/system/bin/sh\nexec >/dev/console 2>&1\nset -x\n# Only a newly built, account-free CI template has this marker.\n[ -f /data/.nexus-provision ] || exit 1\nlocksettings set-disabled true || exit 1\n[ \"$(locksettings get-disabled)\" = true ] || exit 1\nsettings put global device_provisioned 1 || exit 1\nsettings put secure user_setup_complete 1 || exit 1\nsettings put global device_name 'Nexus Android 11' || exit 1\nam unlock-user 0 ! ! || exit 1\nunlocked=0\nfor attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do\n  if am get-started-user-state 0 | grep -q RUNNING_UNLOCKED; then unlocked=1; break; fi\n  sleep 2\ndone\n[ \"$unlocked\" = 1 ] || exit 1\ncandidates=$(cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.HOME)\nprintf '%s\\n' \"$candidates\"\ncomponent=$(printf '%s\\n' \"$candidates\" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -E '^(com[.]android[.]launcher3|org[.]lineageos[.]trebuchet|com[.]bliss[.]launcher|com[.]farmerbb[.]taskbar)/[A-Za-z0-9_.$]+$' | head -n 1)\n[ -n \"$component\" ] || exit 1\ncmd package set-home-activity --user 0 \"$component\" || exit 1\nrm -f /data/.nexus-provision /data/.nexus-provision-adb.pub /data/misc/adb/adb_keys || exit 1\nif [ -f /nexus-home-check.sh ]; then exec /system/bin/sh /nexus-home-check.sh; else exec /system/bin/sh /system/bin/nexus-home-check.sh; fi\n")
    output += record('TRAILER!!!',[0]*13,b'')
    return gzip.compress(bytes(output),mtime=0) if compressed else bytes(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--ramdisk', required=True)
    parser.add_argument('--agent', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    source, output = Path(args.ramdisk),Path(args.output)
    if output.exists(): raise FileExistsError(output)
    original = source.read_bytes()
    try:
        raw=gzip.decompress(original) if original.startswith(b'\x1f\x8b') else original
        layout=[(name,oct(fields[1]),body[:160].decode(errors='replace') if name.lstrip('./')=='init.rc' else '') for name,fields,body in records(raw)]
        print('::notice title=Ramdisk layout::'+json.dumps(layout[:70]).replace('%','%25'))
        output.write_bytes(patch(original,Path(args.agent).read_bytes()))
    except Exception as error:
        print('::error::Ramdisk bridge installation failed: '+str(error))
        raise
    print(json.dumps({'original_sha256':hashlib.sha256(original).hexdigest(),'patched_sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'integration':'Nexus bridge v1; requires Android userdebug su domain and virtio-console driver; no SELinux disabling'},indent=2))
