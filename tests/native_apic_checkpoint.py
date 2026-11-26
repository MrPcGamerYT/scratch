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
    checks={
        'exit_success':p.returncode==0,
        'native_apic_success':'NATIVE_APIC_OS_PID1_OK cpus=1' in p.stdout,
        'mp_enumerated':'Intel MultiProcessor Specification v1.4' in text,
        'ioapic_enumerated':'I/O APIC' in text or 'IOAPIC' in text,
        'pid1_ready':'NEXUS_USERSPACE_READY' in text,
        'two_pid1_ticks':text.count('NEXUS_USERSPACE_TICK')>=2,
        'no_panic':'Kernel panic' not in text,
        'no_unchecked_msr':'unchecked MSR access error' not in text,
        'lapic_timer_irqs':'lapic_timer_irqs=' in p.stdout and 'lapic_timer_irqs=0 ' not in p.stdout,
    }
    print('::notice::APIC_CHECKS='+json.dumps(checks),flush=True)
    if not all(checks.values()):
        print('::error::APIC_FAILED_CHECKS='+json.dumps([key for key,value in checks.items() if not value]),flush=True)
        for line in text.splitlines():
            if 'MSR' in line or 'NEXUS_USERSPACE' in line:
                print('::notice::APIC_MARKER='+line,flush=True)
    assert all(checks.values()),checks
Path('apic-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'one-CPU Linux APIC enumeration and PID1 clocks, not SMP/Android','samples':samples},indent=2))
