"""Actual Linux APIC boot and per-selected-CPU PID1 migration, not Android."""
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
cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr acpi=off pci=off nmi_watchdog=0 panic=-1 apic=verbose'
samples=[]
matrix=[(count,128*count) for count in range(1,9)]+[(2,256),(2,512)]
for index,(cpus,ram_mib) in enumerate(matrix):
    rd=Path(f'apic-diagnostic-{index+1}.cpio');rd.write_bytes(initramfs(expected_cpus=cpus))
    log=Path(f'apic-linux-{index+1}.log')
    p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd+f' maxcpus={cpus}',str(rd),'--apic-probe',f'--cpus={cpus}',f'--ram-mib={ram_mib}'],capture_output=True,text=True,timeout=45)
    text=log.read_text(errors='replace') if log.exists() else ''
    sample={'sample':index+1,'cpus':cpus,'ram_mib':ram_mib,'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'serial_tail':text[-18000:]}
    samples.append(sample);Path('apic-checkpoint.json').write_text(json.dumps({'verified':False,'samples':samples},indent=2))
    for name,value in sample.items():
        if name!='sample':
            escaped=str(value).replace('%','%25').replace('\r','%0D').replace('\n','%0A')
            print(f'::notice::APIC_{name.upper()}={escaped}',flush=True)
    checks={
        'exit_success':p.returncode==0,
        'native_apic_success':f'NATIVE_APIC_OS_PID1_OK cpus={cpus} ram_bytes={ram_mib*1024*1024} ' in p.stdout,
        'guest_migration':f'NEXUS_SMP_AFFINITY_OK cpus={cpus}' in text,
        'two_online':(f'Brought up 1 node, {cpus} CPU'+('s' if cpus>1 else '')) in text,
        'selected_ram_map':f'-0x{ram_mib*1024*1024-1:016x}] usable' in text,
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
Path('apic-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'1..8 selected CPUs, selected RAM, Linux APIC enumeration, per-CPU PID1 affinity migration and clocks; not Android','samples':samples},indent=2))

# Single-CPU DMA acceptance first. APs are explicitly disallowed for this path.
# Keep the pre-existing 1..8-core initramfs matrix above as a separate regression.
def disk_sequence(prefix,raw,filesystem=False,unclean=False):
    base=Path(prefix+'-base.zlag').resolve();cow=Path(prefix+'-overlay.cow').resolve()
    source_sha=hashlib.sha256(raw.read_bytes()).hexdigest()
    subprocess.run([exe,'import-raw',str(raw),str(base)],check=True,capture_output=True)
    base_sha=hashlib.sha256(base.read_bytes()).hexdigest();results=[]
    for index in range(3):
        rd=Path(f'{prefix}-{index+1}.cpio')
        rd.write_bytes(initramfs(disk_write=index==0,filesystem=filesystem,
                                clean_unmount=not(unclean and index==0),expected_cpus=1))
        log=Path(f'{prefix}-{index+1}.log')
        command='boot-kernel-disk-create' if index==0 else 'boot-kernel-disk-open'
        p=subprocess.run([exe,command,str(kernel),str(log),cmd+' maxcpus=1 virtio_mmio.device=4K@0xd0000000:5',str(rd),str(base),str(cow),
                          '--apic-probe','--cpus=1',f'--ram-mib={128*(index+1)}'],capture_output=True,text=True,timeout=45)
        text=log.read_text(errors='replace') if log.exists() else ''
        marker=('NEXUS_EXT4_FILE_FSYNC_UNMOUNT_MATCH' if index==0 else 'NEXUS_EXT4_REBOOT_FILE_MATCH') if filesystem else (
                'NEXUS_VDA_WRITE_FLUSH_READ_MATCH' if index==0 else 'NEXUS_VDA_REBOOT_READ_MATCH')
        if unclean and index==0:marker='NEXUS_EXT4_FSYNC_UNCLEAN_STOP_READY'
        checks={'exit_success':p.returncode==0,'exact_guest_data_match':marker in text,
                'pid1_ready':'NEXUS_USERSPACE_READY' in text,
                'disk_completions_and_gsi5':'NATIVE_APIC_DISK_OK' in p.stdout,
                'virtio_blk_bound':'virtio_blk virtio0: [vda]' in text,
                'no_disk_error':'EXT4-fs error' not in text and 'I/O error' not in text,
                'base_unchanged':hashlib.sha256(base.read_bytes()).hexdigest()==base_sha,
                'source_unchanged':hashlib.sha256(raw.read_bytes()).hexdigest()==source_sha}
        if unclean and index==1:checks['journal_replayed']='recovery complete' in text
        results.append({'sample':index+1,'prefix':prefix,'marker':marker,'checks':checks,'stdout':p.stdout,'stderr':p.stderr,'serial_tail':text[-8000:]})
        Path(prefix+'-checkpoint.json').write_text(json.dumps({'verified':False,'samples':results},indent=2))
        print('::notice::APIC_DISK_CHECKS='+json.dumps({'prefix':prefix,'sample':index+1,**checks}),flush=True)
        if not all(checks.values()):
            error=json.dumps(results[-1]).replace('%','%25').replace('\r','%0D').replace('\n','%0A')
            print('::error::APIC_DISK_FAILURE='+error,flush=True)
        assert all(checks.values()),checks
    Path(prefix+'-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'single-CPU real Linux VirtIO/IOAPIC disk persistence, not SMP DMA or host crash','samples':results},indent=2))

raw=Path('apic-raw.img').resolve();raw.write_bytes(bytes(2*1024*1024))
disk_sequence('apic-block',raw)
filesystem=Path('apic-kernel-artifact/filesystem.ext4').resolve()
checksums={line.split()[1]:line.split()[0] for line in Path('apic-kernel-artifact/SHA256SUMS.txt').read_text().splitlines()}
assert hashlib.sha256(filesystem.read_bytes()).hexdigest()==checksums['filesystem.ext4']
disk_sequence('apic-ext4-clean',filesystem,filesystem=True)
disk_sequence('apic-ext4-unclean',filesystem,filesystem=True,unclean=True)
