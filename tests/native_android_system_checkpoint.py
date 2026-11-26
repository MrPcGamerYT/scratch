"""Boot the original Android-x86 initrd and system with a new native COW layer.
Success is Android's second-stage init marker, NOT framework, Home or ANR success.
"""
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda:source.read(4194304),b''): h.update(chunk)
    return h.hexdigest()


def main():
    root=Path('android-system-artifact');manifest=json.loads((root/'system-manifest.json').read_text())
    for name,info in manifest['files'].items():
        if name not in ['android-system.zlag','kernel','initrd.img','manifest.json']:
            raise ValueError('Unexpected native Android asset name')
        path=root/name
        assert path.is_file() and not path.is_symlink() and path.stat().st_size==info['bytes']
        assert digest(path)==info['sha256'],name
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    log=Path('android-system-serial.log')
    cmd=('earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 androidboot.console=ttyS0 '
         'ignore_loglevel nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1 '
         'root=/dev/ram0 ROOT=/dev/vda SRC=/android virtio_mmio.device=4K@0xd0000000:5 '
         'DEBUG=0 SWITCH=switch_root rdinit=/bin/busybox -- sh -x /init')
    p=subprocess.run([exe,'probe-android-init-create',str(root/'kernel'),str(log),cmd,str(root/'initrd.img'),
                      str(root/'android-system.zlag'),'android-system.cow','--ram-mib=2048',
                      '--framebuffer=android-system.ppm'],capture_output=True,text=True,timeout=150)
    serial=log.read_text(errors='replace') if log.exists() else ''
    timing=Path(str(log)+'.timing.json');metrics=json.loads(timing.read_text()) if timing.exists() else {}
    result={'schema':1,'android_second_stage_init_verified':False,'android_framework_verified':False,
            'android_home_verified':False,'original_init_shell_trace':True,'engine_exit':p.returncode,'stderr':p.stderr,'metrics':metrics,
            'serial_tail':serial[-12000:],'base_sha256_before':manifest['files']['android-system.zlag']['sha256'],
            'base_sha256_after':digest(root/'android-system.zlag')}
    Path('android-system-checkpoint.json').write_text(json.dumps(result,indent=2))
    print(json.dumps(result,indent=2),flush=True)
    print('::notice::ANDROID_SYSTEM_METRICS='+json.dumps(metrics),flush=True)
    for label,text in [('STDERR',p.stderr),('SERIAL_TAIL',serial[-12000:])]:
        for at in range(0,len(text),3000):
            message=f'ANDROID_SYSTEM_{label}_{at}='+text[at:at+3000]
            print('::notice::'+message.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
    assert result['base_sha256_before']==result['base_sha256_after'],'Base disk changed'
    assert p.returncode==0,(p.stderr,serial[-4000:])
    assert 'Found at /dev/vda' in serial and 'virtio_blk virtio0: [vda]' in serial,serial[-4000:]
    assert metrics.get('android_init_second_stage_ms') is not None,serial[-4000:]
    assert metrics.get('irq5_injections',0)>0
    assert metrics['total_boot_verified'] is False and metrics['android_home_ready_ms'] is None
    result['android_second_stage_init_verified']=True
    Path('android-system-checkpoint.json').write_text(json.dumps(result,indent=2))
    print('::notice::ANDROID_INIT_ACCEPTANCE='+json.dumps({'original_initrd':True,'original_system_payload':True,
          'native_virtio_root':True,'second_stage_init':True,'base_unchanged':True,'android_framework':False,'android_home':False}),flush=True)


if __name__=='__main__':
    try:
        main()
    except Exception as error:
        print('::error::'+str(error)[-3000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        raise
