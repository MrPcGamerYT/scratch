"""Opt-in actual Linux APIC probe; intentionally one CPU before SMP integration."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from native_initramfs import initramfs

exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
kernel=Path('apic-kernel-artifact/bzImage').resolve()
config=Path('apic-kernel-artifact/kernel.config').read_text()
for option in ('SMP','X86_LOCAL_APIC','X86_IO_APIC','X86_MPPARSE'):
    assert f'CONFIG_{option}=y' in config,option
expected=Path('apic-kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
rd=Path('apic-diagnostic.cpio');rd.write_bytes(initramfs())
cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr acpi=off pci=off maxcpus=1 nmi_watchdog=0 panic=-1 apic=verbose'
samples=[]
for index in range(8):
    log=Path(f'apic-linux-{index+1}.log')
    p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd,str(rd),'--apic-probe'],capture_output=True,text=True,timeout=45)
    text=log.read_text(errors='replace') if log.exists() else ''
    sample={'sample':index+1,'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'serial_tail':text[-18000:]}
    samples.append(sample);Path('apic-checkpoint.json').write_text(json.dumps({'verified':False,'samples':samples},indent=2))
    for name,value in sample.items():
        if name!='sample':
            escaped=str(value).replace('%','%25').replace('\r','%0D').replace('\n','%0A')
            print(f'::notice::APIC_{name.upper()}={escaped}',flush=True)
    assert p.returncode==0,(p.stderr,text[-8000:])
    assert 'NATIVE_APIC_OS_PID1_OK cpus=1' in p.stdout
    assert 'Intel MultiProcessor Specification v1.4' in text
    assert 'I/O APIC' in text or 'IOAPIC' in text
    assert 'NEXUS_USERSPACE_READY' in text
    assert text.count('NEXUS_USERSPACE_TICK')>=2
    assert 'Kernel panic' not in text
    assert 'unchecked MSR access error' not in text
    assert 'lapic_timer_irqs=0 ' not in p.stdout
    assert 'lapic_timer_irqs=' in p.stdout
Path('apic-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'one-CPU Linux APIC enumeration and PID1 clocks, not SMP/Android','samples':samples},indent=2))
