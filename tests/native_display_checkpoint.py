"""Real Linux /dev/fb0 software scanout -> WHP surface -> exact host capture.
No host window, virtual GPU, 3D acceleration, Android or timing certification.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from native_initramfs import initramfs


def main():
    config=Path('kernel-artifact/kernel.config').read_text()
    required=['FB','FB_SIMPLE','SYSFB_SIMPLEFB']
    if any(f'CONFIG_{name}=y' not in config for name in required):
        raise RuntimeError('Display acceptance requires the simplefb-enabled test kernel')
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    kernel=Path('kernel-artifact/bzImage').resolve()
    expected=Path('kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
    assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
    rd=Path('display-initramfs.cpio');rd.write_bytes(initramfs(display=True))
    reference=b'P6\n800 600\n255\n'+bytes(component for y in range(600) for x in range(800) for component in ((x*7)%256,(x*5+y)%256,(x*3)%256))
    reference=reference[:-3]+bytes([0xbe,0xad,0xde])  # guest mmap sentinel, independent of write() rows
    samples=[]
    for index in range(3):
        log=Path(f'display-serial-{index+1}.log');capture=Path(f'display-{index+1}.ppm')
        cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
        p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd,str(rd),f'--framebuffer={capture}',
                          '--ram-mib=256'],capture_output=True,text=True,timeout=45)
        serial=log.read_text(errors='replace') if log.exists() else ''
        pixels=capture.read_bytes() if capture.exists() else b''
        timing=Path(str(log)+'.timing.json')
        metrics=json.loads(timing.read_text()) if timing.exists() else {}
        sample={'sample':index+1,'engine_exit':p.returncode,'stderr':p.stderr,'serial_tail':serial[-3500:],
                'capture_sha256':hashlib.sha256(pixels).hexdigest(),'exact_pixels':pixels==reference,'metrics':metrics}
        samples.append(sample)
        Path('display-checkpoint.json').write_text(json.dumps({'verified':False,'samples':samples},indent=2))
        print(json.dumps(sample,indent=2),flush=True)
        assert p.returncode==0,(p.stderr,serial[-3500:])
        assert 'NEXUS_FB0_PATTERN_WRITTEN' in serial,serial[-3500:]
        assert pixels==reference,('Guest/host framebuffer pixel mismatch',len(pixels))
        assert metrics.get('linux_userspace_ready_ms') is not None
        assert metrics['total_boot_verified'] is False and metrics['android_home_ready_ms'] is None
        refusal=Path(f'display-refusal-{index+1}.log')
        blocked=subprocess.run([exe,'boot-kernel',str(kernel),str(refusal),cmd,str(rd),f'--framebuffer={capture}'],capture_output=True,text=True,timeout=15)
        assert blocked.returncode!=0 and capture.read_bytes()==reference
        refused_metrics=json.loads(Path(str(refusal)+'.timing.json').read_text())
        assert refused_metrics['vcpu_run_requested_ms'] is None and refusal.read_bytes()==b''

    result={'verified':True,'scope':'800x600 XRGB8888 software scanout only; no accelerated GPU or player window',
            'kernel_sha256':expected,'reference_sha256':hashlib.sha256(reference).hexdigest(),'samples':samples}
    Path('display-checkpoint.json').write_text(json.dumps(result,indent=2))
    print('::notice::DISPLAY_ACCEPTANCE='+json.dumps({'guest_fbdev_write':True,'guest_fbdev_mmap':True,'exact_host_pixels':True,'repetitions':3,'existing_capture_refused_before_vcpu':True,'accelerated_gpu':False,'android_ready':False}),flush=True)


if __name__=='__main__':
    try:
        main()
    except Exception as error:
        print('::error::'+str(error)[-3000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        raise
