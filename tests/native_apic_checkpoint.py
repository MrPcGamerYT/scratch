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

# Disk DMA acceptance: vary CPUs across reopen, including queue access from APs.
# Keep the pre-existing 1..8-core initramfs matrix above as a separate regression.
def disk_sequence(prefix,raw,filesystem=False,unclean=False):
    base=Path(prefix+'-base.zlag').resolve();cow=Path(prefix+'-overlay.cow').resolve()
    source_sha=hashlib.sha256(raw.read_bytes()).hexdigest()
    subprocess.run([exe,'import-raw',str(raw),str(base)],check=True,capture_output=True)
    base_sha=hashlib.sha256(base.read_bytes()).hexdigest();results=[]
    profiles=([(8,1024),(4,512),(2,256)] if unclean else [(4,512),(2,256),(8,1024)]) if filesystem else [(1,128),(2,256),(8,1024)]
    for index,(cpus,ram_mib) in enumerate(profiles):
        rd=Path(f'{prefix}-{index+1}.cpio')
        rd.write_bytes(initramfs(disk_write=index==0,filesystem=filesystem,
                                clean_unmount=not(unclean and index==0),expected_cpus=cpus,bridge=True))
        log=Path(f'{prefix}-{index+1}.log')
        command='boot-kernel-disk-create' if index==0 else 'boot-kernel-disk-open'
        p=subprocess.run([exe,command,str(kernel),str(log),cmd+f' maxcpus={cpus} virtio_mmio.device=4K@0xd0000000:5',str(rd),str(base),str(cow),
                          '--apic-probe',f'--cpus={cpus}',f'--ram-mib={ram_mib}','--bridge-probe'],capture_output=True,text=True,timeout=45)
        text=log.read_text(errors='replace') if log.exists() else ''
        marker=('NEXUS_EXT4_FILE_FSYNC_UNMOUNT_MATCH' if index==0 else 'NEXUS_EXT4_REBOOT_FILE_MATCH') if filesystem else (
                'NEXUS_VDA_WRITE_FLUSH_READ_MATCH' if index==0 else 'NEXUS_VDA_REBOOT_READ_MATCH')
        if unclean and index==0:marker='NEXUS_EXT4_FSYNC_UNCLEAN_STOP_READY'
        checks={'exit_success':p.returncode==0,'exact_guest_data_match':marker in text,
                'pid1_ready':'NEXUS_USERSPACE_READY' in text,
                'disk_completions_and_gsi5':'NATIVE_APIC_DISK_OK' in p.stdout and 'dma_rendezvous=' in p.stdout,
                'selected_cpu_migration':f'NEXUS_SMP_AFFINITY_OK cpus={cpus}' in text,
                'effective_config':f'cpus={cpus} ram_bytes={ram_mib*1024*1024} ' in p.stdout,
                'com2_exact_query':'NEXUS_PRIVATE_COM2_EXACT_QUERY_MATCH' in text,
                'com2_host_result':'NATIVE_APIC_BRIDGE_OK app=org.nexus.fixture ' in p.stdout,
                'virtio_blk_bound':'virtio_blk virtio0: [vda]' in text,
                'no_disk_error':'EXT4-fs error' not in text and 'I/O error' not in text,
                'base_unchanged':hashlib.sha256(base.read_bytes()).hexdigest()==base_sha,
                'source_unchanged':hashlib.sha256(raw.read_bytes()).hexdigest()==source_sha}
        if unclean and index==1:checks['journal_replayed']='recovery complete' in text
        results.append({'sample':index+1,'cpus':cpus,'ram_mib':ram_mib,'prefix':prefix,'marker':marker,'checks':checks,'stdout':p.stdout,'stderr':p.stderr,'serial_tail':text[-8000:]})
        Path(prefix+'-checkpoint.json').write_text(json.dumps({'verified':False,'samples':results},indent=2))
        print('::notice::APIC_DISK_CHECKS='+json.dumps({'prefix':prefix,'sample':index+1,**checks}),flush=True)
        if not all(checks.values()):
            error=json.dumps(results[-1]).replace('%','%25').replace('\r','%0D').replace('\n','%0A')
            print('::error::APIC_DISK_FAILURE='+error,flush=True)
        assert all(checks.values()),checks
    Path(prefix+'-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'1/2/4/8-CPU Linux VirtIO/IOAPIC DMA rendezvous, persistence and journal replay; not host crash or concurrent multi-process stress','samples':results},indent=2))

raw=Path('apic-raw.img').resolve();raw.write_bytes(bytes(2*1024*1024))
disk_sequence('apic-block',raw)
filesystem=Path('apic-kernel-artifact/filesystem.ext4').resolve()
checksums={line.split()[1]:line.split()[0] for line in Path('apic-kernel-artifact/SHA256SUMS.txt').read_text().splitlines()}
assert hashlib.sha256(filesystem.read_bytes()).hexdigest()==checksums['filesystem.ext4']
disk_sequence('apic-ext4-clean',filesystem,filesystem=True)
disk_sequence('apic-ext4-unclean',filesystem,filesystem=True,unclean=True)

# Faults occur while the exclusive RAM gate is held, BEFORE queue service.
# Never inject into user/previous overlays: every fault uses disk-create + a new COW.
import time
fault_raw=Path('apic-fault-seed.img').resolve()
seed=bytearray(2*1024*1024);seed[4096:4608]=bytes((n*37+9)%256 for n in range(512));fault_raw.write_bytes(seed)
fault_base=Path('apic-fault-seed.zlag').resolve()
subprocess.run([exe,'import-raw',str(fault_raw),str(fault_base)],check=True,capture_output=True)
fault_base_sha=hashlib.sha256(fault_base.read_bytes()).hexdigest()
fault_rd=Path('apic-fault.cpio');fault_rd.write_bytes(initramfs(disk_write=False))
fault_samples=[]
for cpus in (2,4,8):
    for mode in ('error','panic','error-after','panic-after'):
        prefix=f'apic-fault-{cpus}-{mode}';cow=Path(prefix+'.cow').resolve();log=Path(prefix+'.log')
        started=time.monotonic()
        p=subprocess.run([exe,'boot-kernel-disk-create',str(kernel),str(log),cmd+f' maxcpus={cpus} virtio_mmio.device=4K@0xd0000000:5',
                          str(fault_rd),str(fault_base),str(cow),'--apic-probe',f'--cpus={cpus}',f'--test-dma-failure={mode}'],
                         capture_output=True,text=True,timeout=45)
        elapsed=time.monotonic()-started
        checks={'failed_as_requested':p.returncode!=0,
                'injected_exclusively':f'NATIVE_DMA_FAILURE_INJECTED kind={mode.title().replace("-","")}' in p.stderr and f'exclusive=true before_service={str(not mode.endswith("-after")).lower()}' in p.stderr,
                'targeted_worker':f'cpu={cpus-1 if mode.endswith("-after") else 0} cpus={cpus}' in p.stderr,
                'all_aps_started':f'cpus={cpus} started_aps={cpus-1} ' in p.stderr,
                'workers_joined_and_gate_released':'NATIVE_APIC_CLEANUP ap_workers=0 ram_gate_released=true' in p.stderr,
                'vm_dropped':'NATIVE_VM_RESOURCES_DROPPED' in p.stderr,
                'no_teardown_api_error':'NATIVE_TEARDOWN_ERROR' not in p.stderr,
                'no_false_boot_success':'NATIVE_APIC_OS_PID1_OK' not in p.stdout,
                'base_unchanged':hashlib.sha256(fault_base.read_bytes()).hexdigest()==fault_base_sha}
        if mode.endswith('-after'):checks['completed_real_work']='completed=0' not in p.stderr and 'completed=' in p.stderr
        result={'cpus':cpus,'mode':mode,'seconds':elapsed,'checks':checks,'stderr':p.stderr,'returncode':p.returncode}
        fault_samples.append(result)
        Path('apic-failure-checkpoint.json').write_text(json.dumps({'verified':False,'samples':fault_samples},indent=2))
        print('::notice::APIC_FAILURE_CHECKS='+json.dumps({'cpus':cpus,'mode':mode,**checks}),flush=True)
        if not all(checks.values()):
            print('::error::APIC_FAILURE_DETAIL='+json.dumps(result).replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        assert all(checks.values()),checks
        # A fresh SMP VM must be able to reopen the failed VM's overlay and read
        # the seeded exact data, not just create another unrelated blank guest.
        rd=Path(prefix+'-reopen.cpio');rd.write_bytes(initramfs(disk_write=False,expected_cpus=cpus))
        reopened=Path(prefix+'-reopen.log')
        p=subprocess.run([exe,'boot-kernel-disk-open',str(kernel),str(reopened),cmd+f' maxcpus={cpus} virtio_mmio.device=4K@0xd0000000:5',
                          str(rd),str(fault_base),str(cow),'--apic-probe',f'--cpus={cpus}'],capture_output=True,text=True,timeout=45)
        text=reopened.read_text(errors='replace') if reopened.exists() else ''
        recovery={'exit_success':p.returncode==0,'data_match':'NEXUS_VDA_REBOOT_READ_MATCH' in text,
                  'migrated_all_cpus':f'NEXUS_SMP_AFFINITY_OK cpus={cpus}' in text,
                  'real_disk_irqs':'NATIVE_APIC_DISK_OK' in p.stdout,
                  'base_unchanged':hashlib.sha256(fault_base.read_bytes()).hexdigest()==fault_base_sha}
        result['recovery']=recovery;result['recovery_stderr']=p.stderr
        Path('apic-failure-checkpoint.json').write_text(json.dumps({'verified':False,'samples':fault_samples},indent=2))
        if not all(recovery.values()):
            print('::error::APIC_FAILURE_RECOVERY='+json.dumps({'checks':recovery,'stderr':p.stderr,'serial_tail':text[-6000:]}).replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        assert all(recovery.values()),recovery
Path('apic-failure-checkpoint.json').write_text(json.dumps({'verified':True,'scope':'12 BSP-before/AP-after-service DMA error/panic teardowns and same-overlay SMP recoveries; not mid-write, reset or host-power-loss certification','samples':fault_samples},indent=2))
print('::notice::APIC_FAILURE_ACCEPTANCE 12 injected errors/panics, joined workers, released RAM gate, WHP cleanup and same-overlay SMP reboots PASS',flush=True)
