"""Actual source-built Linux image through WHP. Early banner != complete boot."""
import hashlib
from pathlib import Path
import subprocess

exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
kernel=Path('kernel-artifact/bzImage').resolve()
expected=Path('kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
p=subprocess.run([exe,'inspect-kernel',str(kernel)],capture_output=True,text=True,timeout=20)
print(p.stdout,p.stderr,flush=True)
if p.returncode:
    print('::error::Kernel header validation failed: '+p.stderr.replace('\n','%0A'),flush=True)
    raise SystemExit(1)
cmd='earlyprintk=serial,ttyS0,115200,keep earlycon=uart8250,io,0x3f8,115200n8 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
p=subprocess.run([exe,'boot-kernel',str(kernel),'kernel-serial.log',cmd],capture_output=True,text=True,timeout=45)
serial=Path('kernel-serial.log').read_text(errors='replace') if Path('kernel-serial.log').exists() else ''
report=f'engine_exit={p.returncode}\nfull_boot_verified=false\nkernel_sha256={expected}\nstdout={p.stdout}\nstderr={p.stderr}\nserial={serial}\n'
Path('kernel-checkpoint.txt').write_text(report,encoding='utf-8')
print(report,flush=True)
assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
if 'Linux version 6.6.0' not in serial:
    text=(p.stderr+'\n'+serial[-2200:]).replace('%','%25').replace('\r','%0D').replace('\n','%0A')
    print('::error::Actual Linux early-banner checkpoint NOT reached: '+text,flush=True)
    raise SystemExit(1)
print('::notice::Actual pinned Linux v6.6 early version banner reached via independent WHP loader. Full boot=false; no Android/GPU/performance certification. Blocker: '+p.stderr[-1500:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
