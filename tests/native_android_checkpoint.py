"""Original Android-distribution kernel on independent WHP, diagnostic PID1 only.
This gate must not be described as Android init/framework/Home boot.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from native_initramfs import initramfs


def notice(text):
    print('::notice::'+text.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)


def main():
    assets=Path('android-boot-assets');manifest=json.loads((assets/'manifest.json').read_text())
    def verify_sources():
        for name,info in manifest['files'].items():
            data=(assets/name).read_bytes()
            assert len(data)==info['bytes'] and hashlib.sha256(data).hexdigest()==info['sha256']
    verify_sources()
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    kernel=(assets/'kernel').resolve()
    inspected=subprocess.run([exe,'inspect-kernel',str(kernel)],capture_output=True,text=True,timeout=20)
    notice('ANDROID_KERNEL_INSPECT='+inspected.stdout+inspected.stderr)
    assert inspected.returncode==0,inspected.stderr
    rd=Path('android-diagnostic-initramfs.cpio');rd.write_bytes(initramfs())
    samples=[]
    for index in range(3):
        log=Path(f'android-kernel-{index+1}.log');surface=Path(f'android-kernel-{index+1}.ppm')
        cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
        p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd,str(rd),
                          '--ram-mib=1024',f'--framebuffer={surface}'],capture_output=True,text=True,timeout=45)
        serial=log.read_text(errors='replace') if log.exists() else ''
        timing=Path(str(log)+'.timing.json')
        metrics=json.loads(timing.read_text()) if timing.exists() else {}
        samples.append({'sample':index+1,'engine_exit':p.returncode,'stderr':p.stderr,'serial_tail':serial[-6000:],
                        'metrics':metrics,'serial_sha256':hashlib.sha256(log.read_bytes()).hexdigest() if log.exists() else None})
        report={'schema':1,'android_kernel_diagnostic_verified':False,'android_init_executed':False,
                'android_framework_verified':False,'android_home_verified':False,
                'source_kernel_sha256':manifest['files']['kernel']['sha256'],'samples':samples}
        Path('android-kernel-checkpoint.json').write_text(json.dumps(report,indent=2))
        print(json.dumps(samples[-1],indent=2),flush=True)
        notice('ANDROID_KERNEL_STDERR='+p.stderr[-3000:])
        notice('ANDROID_KERNEL_SERIAL_TAIL='+serial[-6000:])
        verify_sources()
        assert p.returncode==0,(p.stderr,serial[-6000:])
        assert 'Linux version ' in serial and 'NEXUS_USERSPACE_PID1' in serial
        assert serial.count('NEXUS_USERSPACE_TICK')==2 and 'NEXUS_USERSPACE_READY' in serial
        assert metrics.get('linux_userspace_ready_ms') is not None and metrics.get('irq0_injections',0)>0
        assert metrics['total_boot_verified'] is False and metrics['android_home_ready_ms'] is None
    report['android_kernel_diagnostic_verified']=True
    Path('android-kernel-checkpoint.json').write_text(json.dumps(report,indent=2))
    notice('ANDROID_KERNEL_ACCEPTANCE='+json.dumps({'original_kernel_diagnostic_pid1':True,'repetitions':3,
            'kernel_sha256':manifest['files']['kernel']['sha256'],'android_init':False,'android_framework':False,'android_home':False}))


if __name__=='__main__':
    try:
        main()
    except Exception as error:
        print('::error::'+str(error)[-4000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        raise
