"""Real Linux PIT bring-up and timing evidence. NOT a completed-boot benchmark."""
import base64
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time
import traceback


def notice(text):
    print('::notice::'+text.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)


def main():
    exe=str(Path('rust/nexus-engine/target/release/nexus-engine.exe').resolve())
    kernel=Path('kernel-artifact/bzImage').resolve()
    expected=Path('kernel-artifact/SHA256SUMS.txt').read_text().split()[0]
    assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
    inspected=subprocess.run([exe,'inspect-kernel',str(kernel)],capture_output=True,text=True,timeout=20)
    assert inspected.returncode==0,inspected.stderr
    cmd='earlyprintk=serial,ttyS0,115200,keep earlycon=uart8250,io,0x3f8,115200n8 console=ttyS0,115200 nokaslr noapic nolapic acpi=off pci=off maxcpus=1 panic=-1'
    samples=[]
    for index in range(3):
        log=Path('kernel-serial.log' if index==0 else f'kernel-serial-{index+1}.log')
        start=time.perf_counter_ns()
        p=subprocess.run([exe,'boot-kernel',str(kernel),str(log),cmd],capture_output=True,text=True,timeout=45)
        launch_to_exit_ms=(time.perf_counter_ns()-start)/1_000_000
        raw=log.read_bytes() if log.exists() else b''
        serial=raw.decode(errors='replace')
        metrics_path=Path(str(log)+'.timing.json')
        metrics=json.loads(metrics_path.read_text()) if metrics_path.exists() else {}
        sample={'sample':index+1,'kind':'first_process_launch' if index==0 else 'repeat_process_launch',
            'host_cache_state':'uncontrolled; NOT a cold-cache measurement','engine_exit':p.returncode,
            'launch_to_engine_exit_ms':launch_to_exit_ms,'full_boot_verified':False,
            'kernel_sha256':expected,'serial_sha256':hashlib.sha256(raw).hexdigest(),'serial_bytes':len(raw),
            'stderr':p.stderr,'metrics':metrics}
        samples.append(sample)
        Path('kernel-checkpoint.txt').write_text(json.dumps(samples,indent=2),encoding='utf-8')
        print(json.dumps(sample,indent=2),flush=True)
        assert 'Linux version 6.6.0' in serial,('Linux banner not reached',p.stderr,serial[-1800:])
        assert metrics,'No native monotonic timing report'
        assert metrics['pit_channel2_programs']>0 and metrics['pit_accesses']>0,metrics
        assert 'Unsupported kernel port 0x61' not in p.stderr,p.stderr
        assert not any(word in metrics['stop_reason'].lower() for word in ['deadline','exit budget']),metrics
        assert metrics['pic_accesses']>0,metrics
        assert 'Unsupported kernel port 0xa1' not in p.stderr,p.stderr
        assert 'Unsupported kernel port 0x70' not in p.stderr,p.stderr
        assert metrics['total_boot_verified'] is False
        assert metrics['target_met'] is None and metrics['android_home_ready_ms'] is None
        assert metrics['android_home_target_ms']==60000
        fields=['header_validated_ms','vm_ready_ms','vcpu_run_requested_ms','linux_banner_ms','stopped_ms']
        times=[metrics[field] for field in fields]
        assert all(isinstance(n,int) and n>=0 for n in times) and times==sorted(times),times
        assert metrics['serial_bytes']==len(raw)
        assert hashlib.sha256(kernel.read_bytes()).hexdigest()==expected
        if index==0:
            notice('KERNEL_SERIAL_PREFIX_BASE64='+base64.b64encode(raw[:2048]).decode())
            notice(f'KERNEL_SHA256={expected}; SERIAL_SHA256={hashlib.sha256(raw).hexdigest()}; SERIAL_BYTES={len(raw)}')
        notice(f'Linux PIT checkpoint sample {index+1}: banner={metrics["linux_banner_ms"]} ms from engine main; PIT programs={metrics["pit_channel2_programs"]}; stopped={metrics["stopped_ms"]} ms; FULL BOOT=false; blocker: '+p.stderr[-1300:])
    summary={'schema':1,'target':'approximately 60 seconds from emulator launch to usable, verified Android Home',
        'target_ms':60000,'target_verified':False,'android_home_ready_ms':None,
        'scope':'hosted Windows Server 2022, tiny Linux early boot only; NOT Windows 10/Android boot performance',
        'linux_banner_median_ms':statistics.median(s['metrics']['linux_banner_ms'] for s in samples),
        'samples':samples}
    Path('boot-timing-summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    notice('Three actual Linux PIT/timing checkpoints passed. Android total-boot target remains UNVERIFIED; no full boot/GPU/FPS claim.')


if __name__=='__main__':
    try:
        main()
    except Exception as exc:
        traceback.print_exc()
        print('::error::'+str(exc)[-3000:].replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
        raise SystemExit(1)
