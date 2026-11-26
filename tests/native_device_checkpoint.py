"""Real Linux private-COM2 native bridge query. Not Android/package/network/audio proof."""
import hashlib
import json
from pathlib import Path
import subprocess
from native_initramfs import initramfs


def main():
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    kernel=Path('kernel-artifact/bzImage')
    assert hashlib.sha256(kernel.read_bytes()).hexdigest()==Path('kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
    rd=Path('device-bridge-initramfs.cpio');rd.write_bytes(initramfs(bridge=True))
    samples=[]
    for index in range(3):
        log=Path(f'device-bridge-{index}.log')
        cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
        p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd,str(rd),'--ram-mib=256','--bridge-probe'],capture_output=True,text=True,timeout=150)
        serial=log.read_text(errors='replace') if log.exists() else ''
        timing=Path(str(log)+'.timing.json');metrics=json.loads(timing.read_text()) if timing.exists() else {}
        sample={'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'serial_tail':serial[-4000:],'metrics':metrics}
        samples.append(sample);Path('native-device-checkpoint.json').write_text(json.dumps({'verified':False,'samples':samples},indent=2))
        print(json.dumps(sample),flush=True)
        if p.returncode:
            print('::error::'+str(sample)[-3000:].replace('%','%25').replace('\n','%0A'),flush=True)
        assert p.returncode==0,sample
        assert 'NEXUS_PRIVATE_COM2_EXACT_QUERY_MATCH' in serial
        assert 'NATIVE_BRIDGE_QUERY_OK app=org.nexus.fixture transport=private-COM2' in p.stdout
        assert metrics['irq3_injections']>0 and metrics['android_home_ready_ms'] is None and metrics['total_boot_verified'] is False
    Path('native-device-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'Linux private serial bridge query only','samples':samples,'android_bridge_verified':False,'apk_install_verified':False,'network_verified':False,'audio_verified':False},indent=2))
    print('::notice::NATIVE_DEVICE_BRIDGE: Linux ttyS1 raw bidirectional query/response and IRQ3 PASS, three boots; no QEMU/ADB/socket; NOT Android/app installation/network/audio verification',flush=True)


if __name__=='__main__': main()
