"""Real Linux PIT bring-up and timing evidence. NOT a completed-boot benchmark."""
import base64
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time
import traceback
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from native_initramfs import initramfs


def notice(text):
    print('::notice::'+text.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)


def filesystem_checkpoint(exe,kernel,cmd,config,unclean=False):
    if 'CONFIG_EXT4_FS=y' not in config:
        return {'verified':False,'reason':'legacy kernel lacks EXT4_FS'}
    raw=Path('kernel-artifact/filesystem.ext4').resolve()
    checksums={line.split()[1]:line.split()[0] for line in Path('kernel-artifact/SHA256SUMS.txt').read_text().splitlines()}
    assert raw.is_file() and hashlib.sha256(raw.read_bytes()).hexdigest()==checksums['filesystem.ext4']
    prefix='filesystem-unclean' if unclean else 'filesystem'
    base=Path(prefix+'-disk.zlag').resolve();cow=Path(prefix+'-disk.cow').resolve()
    subprocess.run([exe,'import-raw',str(raw),str(base)],check=True,capture_output=True)
    digest=hashlib.sha256(base.read_bytes()).hexdigest()
    samples=[]
    for index in range(2 if unclean else 3):
        rd=Path(f'{prefix}-initramfs-{index+1}.cpio')
        rd.write_bytes(initramfs(disk_write=index==0,filesystem=True,clean_unmount=not (unclean and index==0)))
        log=Path(f'kernel-serial-{prefix}-{index+1}.log')
        p=subprocess.run([exe,'boot-kernel-disk-create' if index==0 else 'boot-kernel-disk-open',
                          str(kernel),str(log),cmd,str(rd),str(base),str(cow)],capture_output=True,text=True,timeout=45)
        serial=log.read_text(errors='replace') if log.exists() else ''
        timing=Path(str(log)+'.timing.json')
        metrics=json.loads(timing.read_text()) if timing.exists() else {}
        marker='NEXUS_EXT4_FILE_FSYNC_UNMOUNT_MATCH' if index==0 else 'NEXUS_EXT4_REBOOT_FILE_MATCH'
        if unclean and index==0:
            marker='NEXUS_EXT4_FSYNC_UNCLEAN_STOP_READY'
        sample={'sample':index+1,'unclean_vm_stop':unclean,'operation':('create_write_fsync_dirsync_vm_stop' if unclean else 'create_write_fsync_dirsync_unmount') if index==0 else 'readonly_remount_read_unmount',
                'engine_exit':p.returncode,'marker':marker,'marker_seen':marker in serial,'metrics':metrics,
                'stderr':p.stderr,'serial_sha256':hashlib.sha256(log.read_bytes()).hexdigest() if log.exists() else None,
                'journal_recovery_observed':'recovery complete' in serial,'base_unchanged':hashlib.sha256(base.read_bytes()).hexdigest()==digest}
        samples.append(sample)
        Path(prefix+'-checkpoint.json').write_text(json.dumps({'verified':False,'samples':samples},indent=2))
        notice('EXT4_SAMPLE='+json.dumps(sample))
        notice('EXT4_SERIAL_TAIL='+serial[-3500:])
        assert p.returncode==0 and marker in serial,(p.stderr,serial[-3500:])
        assert 'virtio_blk virtio0: [vda]' in serial and metrics.get('irq5_injections',0)>0
        if unclean and index==1:
            assert 'recovery complete' in serial,('No ext4 journal recovery evidence',serial[-3500:])
        assert 'EXT4-fs error' not in serial and 'I/O error' not in serial,serial[-3500:]
        assert metrics.get('linux_userspace_ready_ms') is not None and metrics['total_boot_verified'] is False
        assert hashlib.sha256(base.read_bytes()).hexdigest()==digest
        assert hashlib.sha256(raw.read_bytes()).hexdigest()==checksums['filesystem.ext4']
    result={'verified':True,'scope':'ext4 file/directory fsync with VM termination while mounted, journal replay and readonly remount; not host crash or Android certification' if unclean else 'ext4 clean file/directory fsync, unmount, two separate guest/process readonly remounts; not host crash or Android certification','source_ext4_sha256':checksums['filesystem.ext4'],'base_zlag_sha256':digest,'samples':samples}
    Path(prefix+'-checkpoint.json').write_text(json.dumps(result,indent=2))
    return result


def main():
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    kernel=Path('kernel-artifact/bzImage').resolve()
    expected=Path('kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
    assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
    inspected=subprocess.run([exe,'inspect-kernel',str(kernel)],capture_output=True,text=True,timeout=20)
    assert inspected.returncode==0,inspected.stderr
    cmd='earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
    rd=Path("diagnostic-initramfs.cpio")
    rd.write_bytes(initramfs())
    rd_sha=hashlib.sha256(rd.read_bytes()).hexdigest()
    config=Path('kernel-artifact/kernel.config').read_text()
    disk_enabled='CONFIG_VIRTIO_BLK=y' in config
    if disk_enabled:
        cmd+=' virtio_mmio.device=4K@0xd0000000:5'
        base=Path('kernel-disk.zlag').resolve();raw_disk=Path('kernel-disk.raw').resolve();cow=Path('kernel-disk.cow').resolve()
        raw_disk.write_bytes(bytes(2*1024*1024))
        subprocess.run([exe,'import-raw',str(raw_disk),str(base)],check=True,capture_output=True)
        base_digest=hashlib.sha256(base.read_bytes()).hexdigest()
    samples=[]
    for index in range(3):
        log=Path('kernel-serial.log' if index==0 else f'kernel-serial-{index+1}.log')
        if disk_enabled:
            rd.write_bytes(initramfs(disk_write=index==0))
            rd_sha=hashlib.sha256(rd.read_bytes()).hexdigest()
        start=time.perf_counter_ns()
        command=[exe,'boot-kernel',str(kernel),str(log),cmd,str(rd)]
        if disk_enabled:
            command=[exe,'boot-kernel-disk-create' if index==0 else 'boot-kernel-disk-open',str(kernel),str(log),cmd,str(rd),str(base),str(cow)]
        ram_mib=[256,512,1024][index]
        command.append(f'--ram-mib={ram_mib}')
        p=subprocess.run(command,capture_output=True,text=True,timeout=45)
        launch_to_exit_ms=(time.perf_counter_ns()-start)/1_000_000
        raw=log.read_bytes() if log.exists() else b''
        serial=raw.decode(errors='replace')
        metrics_path=Path(str(log)+'.timing.json')
        metrics=json.loads(metrics_path.read_text()) if metrics_path.exists() else {}
        sample={'sample':index+1,'kind':'first_process_launch' if index==0 else 'repeat_process_launch',
            'host_cache_state':'uncontrolled; NOT a cold-cache measurement','engine_exit':p.returncode,
            'ram_mib':ram_mib,'virtio_disk_enabled':disk_enabled,'virtio_expected_operation':('write_fsync_read' if index==0 else 'separate_boot_read') if disk_enabled else None,
            'launch_to_engine_exit_ms':launch_to_exit_ms,'full_boot_verified':False,
            'kernel_sha256':expected,'initramfs_sha256':rd_sha,'serial_sha256':hashlib.sha256(raw).hexdigest(),'serial_bytes':len(raw),
            'stderr':p.stderr,'metrics':metrics}
        samples.append(sample)
        Path('kernel-checkpoint.txt').write_text(json.dumps(samples,indent=2),encoding='utf-8')
        print(json.dumps(sample,indent=2),flush=True)
        assert 'Linux version 6.6.0' in serial,('Linux banner not reached',p.stderr,serial[-1800:])
        if disk_enabled:
            notice('VIRTIO_LINUX_STDERR='+p.stderr[-2500:])
            notice('VIRTIO_LINUX_SERIAL_TAIL='+serial[-3000:])
            assert 'virtio_blk virtio0: [vda]' in serial,('No Linux vda discovery',p.stderr,serial[-3000:])
            expected_marker='NEXUS_VDA_WRITE_FLUSH_READ_MATCH' if index==0 else 'NEXUS_VDA_REBOOT_READ_MATCH'
            assert expected_marker in serial,('No Linux block write/fsync/reboot read proof',p.stderr,serial[-3000:])
            assert metrics.get('irq5_injections',0)>0,('No real VirtIO IRQ delivery',metrics)
            assert hashlib.sha256(base.read_bytes()).hexdigest()==base_digest
            notice('Linux virtio-blk vda discovery, IRQ5, block write/fsync/read and separate-boot persistence verified; not filesystem or Android readiness')
        assert metrics,'No native monotonic timing report'
        assert metrics['pit_channel2_programs']>0 and metrics['pit_accesses']>0,metrics
        assert 'Unsupported kernel port 0x61' not in p.stderr,p.stderr
        assert not any(word in metrics['stop_reason'].lower() for word in ['deadline','exit budget']),metrics
        assert metrics['pic_accesses']>0,metrics
        assert metrics['pit0_programs']>0 and metrics['irq0_injections']>0,metrics
        assert 'Unsupported kernel port 0xa1' not in p.stderr,p.stderr
        assert 'Unsupported kernel port 0x70' not in p.stderr,p.stderr
        assert metrics['total_boot_verified'] is False
        assert metrics['target_met'] is None and metrics['android_home_ready_ms'] is None
        assert metrics['android_home_target_ms']==100000
        fields=['header_validated_ms','vm_ready_ms','vcpu_run_requested_ms','linux_banner_ms','linux_userspace_ready_ms','stopped_ms']
        times=[metrics[field] for field in fields]
        assert all(isinstance(n,int) and n>=0 for n in times) and times==sorted(times),times
        assert metrics['serial_bytes']==len(raw)
        assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
        clock_lines='\n'.join(line for line in serial.splitlines() if any(term in line.lower() for term in ['clocksource:','tsc:']))
        notice('CORE_GUEST_CLOCK='+clock_lines)
        assert 'NEXUS_MONOTONIC_4096_NO_BACKWARD_SUBMS_DELTA' in serial,('Guest monotonic clock did not pass fine-resolution sampling',serial[-2200:])
        assert 'clocksource: Switched to clocksource tsc' in serial,('Guest did not select high-resolution TSC',clock_lines)
        assert 'Marking TSC unstable' not in serial,('Guest TSC remains unstable',clock_lines)
        if index==0:
            notice('KERNEL_SERIAL_TAIL_BASE64='+base64.b64encode(raw[-2048:]).decode())
            notice('NATIVE_TIMING='+json.dumps(metrics))
            for offset in range(0,min(len(raw),32768),1500):
                notice(f'KERNEL_SERIAL_CHUNK_{offset:05d}='+base64.b64encode(raw[offset:offset+1500]).decode())
        assert metrics.get('linux_userspace_ready_ms') is not None,('Diagnostic userspace not reached',p.stderr,serial[-2200:])
        if metrics.get('linux_userspace_ready_ms') is not None:
            assert p.returncode==0 and 'NEXUS_USERSPACE_PID1' in serial,serial[-1800:]
            assert serial.count('NEXUS_USERSPACE_TICK')==2,serial[-1800:]
            notice('Linux diagnostic PID1 and two nanosleep returns verified; Android boot remains false')
        if index==0:
            notice('KERNEL_SERIAL_PREFIX_BASE64='+base64.b64encode(raw[:2048]).decode())
            notice(f'KERNEL_SHA256={expected}; SERIAL_SHA256={hashlib.sha256(raw).hexdigest()}; SERIAL_BYTES={len(raw)}')
        notice(f'Linux PIT checkpoint sample {index+1}: banner={metrics["linux_banner_ms"]} ms from engine main; userspace={metrics["linux_userspace_ready_ms"]} ms; IRQ0 injections={metrics["irq0_injections"]}; stopped={metrics["stopped_ms"]} ms; ANDROID BOOT=false; engine output: '+p.stderr[-1300:])
    filesystem=filesystem_checkpoint(exe,kernel,cmd,config) if disk_enabled else {'verified':False,'reason':'no VirtIO drivers'}
    recovery=filesystem_checkpoint(exe,kernel,cmd,config,unclean=True) if disk_enabled else {'verified':False,'reason':'no VirtIO drivers'}
    summary={'schema':1,'target':'approximately 100 seconds from emulator launch to usable, verified Android Home',
        'target_ms':100000,'target_verified':False,'android_home_ready_ms':None,
        'scope':'hosted Windows Server 2022, tiny Linux diagnostic PID1 only; no general Linux distribution; NOT Windows 10/Android boot performance',
        'linux_userspace_checkpoint_verified':True,
        'virtio_linux_block_persistence_verified':disk_enabled,
        'ext4_file_persistence':filesystem,
        'ext4_unclean_vm_recovery':recovery,
        'virtio_scope':'raw block device IO and separate engine/guest boots; no filesystem, crash or Android certification' if disk_enabled else 'legacy kernel regression only; VirtIO drivers absent',
        'linux_userspace_median_ms':statistics.median(s['metrics']['linux_userspace_ready_ms'] for s in samples),
        'linux_banner_median_ms':statistics.median(s['metrics']['linux_banner_ms'] for s in samples),
        'samples':samples}
    Path('boot-timing-summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    notice('Three actual Linux PID1/timer/userspace checkpoints passed; VirtIO block persistence verified='+str(disk_enabled)+'. Android total-boot target remains UNVERIFIED; no full boot/GPU/FPS claim.')


if __name__=='__main__':
    try:
        main()
    except Exception as exc:
        traceback.print_exc()
        print('::error::'+str(exc)[-3000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        raise SystemExit(1)
