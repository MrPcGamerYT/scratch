"""Fail-closed real Android APK gate on the new engine. No installer stubs/ADB/QEMU."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from native_home_evidence import home_evidence
from native_scanout_evidence import scanout_evidence


def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda:f.read(4194304),b''): h.update(b)
    return h.hexdigest()


def main():
    root=Path('native-apk-artifact');manifest=json.loads((root/'SHA256.json').read_text())
    expected={'android-system.zlag','kernel','initrd.img','manifest.json','native-apk-derived-manifest.json','smoke.apk'}
    assert expected<=set(manifest)<=expected|{'smoke.apk.idsig'},'Unexpected/missing APK fixture assets: '+repr(sorted(manifest))
    for name,info in manifest.items():
        path=root/name
        assert path.is_file() and not path.is_symlink() and path.stat().st_size==info['bytes'] and digest(path)==info['sha256'],name
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    log=Path('native-apk-serial.log')
    cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 androidboot.console=ttyS0 ignore_loglevel nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1 clocksource=tsc tsc=reliable root=/dev/ram0 ROOT=/dev/vda SRC=/android HWACCEL=0 virtio_mmio.device=4K@0xd0000000:5 rdinit=/init'
    p=subprocess.run([exe,'probe-android-observe-create',str(root/'kernel'),str(log),cmd,str(root/'initrd.img'),str(root/'android-system.zlag'),'native-apk.cow','--ram-mib=2048','--framebuffer=native-apk-software.ppm',f'--bridge-apk-smoke={root/"smoke.apk"}'],capture_output=True,text=True,timeout=480)
    serial=log.read_text(errors='replace') if log.exists() else ''
    timing=Path(str(log)+'.timing.json');metrics=json.loads(timing.read_text()) if timing.exists() else {}
    result={'schema':1,'diagnostic_engine_deadline_ms':420000,'profile':'HWACCEL=0 software scanout with diagnostic TSC watchdog bypass; not accelerated graphics or general host clock certification','android_apk_verified':False,'step4_complete':False,'anr_fixed':False,'packages_removed':[],
            'bridge_service_connected':'NATIVE_BRIDGE_EVENT HELLO' in p.stdout.splitlines(),
            'android_boot_completed':'NATIVE_BRIDGE_EVENT NEXUS 1 READY 1' in p.stdout.splitlines(),
            'apk_begin_acknowledged':'NATIVE_BRIDGE_EVENT NEXUS 1 APK NEXT' in p.stdout.splitlines(),
            'anr_baseline_captured':all(marker in serial for marker in ['NEXUS_ANR_BASELINE_BEGIN','NEXUS_ANR_BASELINE_END']) and 'NEXUS_ANR_BASELINE_UNAVAILABLE' not in serial,
            'anr_reproduction_verified':False,
            'home_candidate_diagnostics_captured':all(marker in serial for marker in ['NEXUS_HOME_CANDIDATES_BEGIN','NEXUS_HOME_CRASH_LOG_END']),
            'android_home_verified':False,'home_activity_evidence':home_evidence(serial),
            'installer_reported_ok':'NATIVE_BRIDGE_EVENT NEXUS 1 RESULT APK_INSTALL OK' in p.stdout.splitlines(),
            'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'metrics':metrics,'serial_tail':serial[-30000:],'guest_clock_diagnostics':[line for line in serial.splitlines() if any(k in line for k in ['clocksource','tsc:','TSC'])],
            'base_unchanged':digest(root/'android-system.zlag')==manifest['android-system.zlag']['sha256']}
    Path('native-apk-checkpoint.json').write_text(json.dumps(result,indent=2))
    capture=Path('native-apk-software.ppm')
    result['scanout']=scanout_evidence(capture.read_bytes()) if capture.exists() and capture.stat().st_size else None
    Path('native-apk-checkpoint.json').write_text(json.dumps(result,indent=2))
    print('::notice::ANDROID_CLOCK_DIAGNOSTICS='+json.dumps(result['guest_clock_diagnostics']),flush=True)
    print('::notice::ANDROID_HOME_ACTIVITY_EVIDENCE='+json.dumps(result['home_activity_evidence']),flush=True)
    print('::notice::ANDROID_HOME_PROCESS_EVIDENCE='+json.dumps({k:result['home_activity_evidence'][k] for k in ['selected_component','launcher_pid_before','launcher_pid_after','launcher_process_retained','launcher_reported_frames']}),flush=True)
    summary=dict(result['scanout'] or {})
    preview=summary.pop('preview_png_base64','')
    print('::notice::ANDROID_SCANOUT_EVIDENCE='+json.dumps(summary),flush=True)
    for offset in range(0,len(preview),2800):
        print(f'::notice::ANDROID_SCANOUT_PREVIEW_{offset}='+preview[offset:offset+2800],flush=True)
    diagnostic=serial[serial.find('NEXUS_ANR_BASELINE_BEGIN'):] if 'NEXUS_ANR_BASELINE_BEGIN' in serial else ''
    critical='\n'.join(line for line in serial.splitlines() if any(k in line for k in ['clocksource','tsc:','TSC','NEXUS_','Abort message','SurfaceFlinger','F DEBUG','signal 6','nexus-native','nexus-apk','avc: denied']))
    for label,text in [('HOST',p.stdout+'\n'+p.stderr),('ANR_HOME_INVENTORY',diagnostic[:30000]),('HOME_DIAGNOSTICS',serial[serial.find('NEXUS_HOME_CANDIDATES_BEGIN'):serial.find('NEXUS_HOME_CRASH_LOG_END')+len('NEXUS_HOME_CRASH_LOG_END')] if 'NEXUS_HOME_CANDIDATES_BEGIN' in serial else 'Home candidate diagnostics absent'),('BOOT_DIAGNOSTICS',critical[-60000:]),('SERIAL_TAIL',serial[-12000:])]:
        for at in range(0,len(text),2800):
            message=f'REAL_APK_{label}_{at}='+text[at:at+2800]
            print('::notice::'+message.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
    assert result['base_unchanged'],'Source derived disk was mutated'
    assert p.returncode==0,'Real APK gate failed: '+p.stderr[-1000:]
    assert 'NATIVE_BRIDGE_APK_INSTALL_VERIFY_UNINSTALL_OK' in p.stdout
    assert 'NEXUS_ANDROID_REAL_APK_PRESENT' in serial and 'NEXUS_ANDROID_REAL_APK_UNINSTALLED' in serial
    home=result['home_activity_evidence']
    assert home['home_activity_resumed'] and home['crash_capture_complete'],home
    assert home['keyevent_commands_completed'] and home['launcher_process_retained'],home
    assert not home['frame_duration_abort_observed'] and not home['captured_native_crash'],home
    assert metrics['total_boot_verified'] is False and metrics['android_home_ready_ms'] is None
    result['android_apk_verified']=True;Path('native-apk-checkpoint.json').write_text(json.dumps(result,indent=2))
    print('::notice::REAL_APK_ACCEPTANCE: signed benign APK streamed over native COM2, Android pm install, package presence and uninstall verified; not networking/audio/Home/ANR success',flush=True)


if __name__=='__main__':
    try: main()
    except Exception as error:
        print('::error::'+str(error)[-2000:].replace('%','%25').replace('\n','%0A'),flush=True)
        raise
