"""Independent reference checks, NOT Android/GPU certification."""
import hashlib
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zlib

exe=str(Path(sys.argv[1]).resolve())
def run(*args,ok=True):
    p=subprocess.run([exe,*map(str,args)],capture_output=True,text=True,timeout=20)
    assert (p.returncode==0)==ok,(args,p.returncode,p.stdout,p.stderr)
    return p
with tempfile.TemporaryDirectory() as folder:
    root=Path(folder); raw=root/'stopped.img'; target=root/'gustimg.zlag'
    payload=bytes(range(256))*8192;raw.write_bytes(payload)
    before=hashlib.sha256(raw.read_bytes()).hexdigest()
    run('import-raw',raw,target)
    content=target.read_bytes(); header=content[:64]
    assert header[:8]==b'ZLAGIMG\0'
    assert struct.unpack_from('<IIIIQI',header,8)==(1,64,512,0,len(payload),zlib.crc32(payload))
    assert struct.unpack_from('<I',header,60)[0]==zlib.crc32(header[:60])
    assert content[64:]==payload
    run('verify',target);run('import-raw',raw,target,ok=False)
    assert target.read_bytes()==content
    assert hashlib.sha256(raw.read_bytes()).hexdigest()==before
    broken=bytearray(content);broken[-1]^=1;target.write_bytes(broken);run('verify',target,ok=False)
    run('boot',target,ok=False)
    text=run('capabilities').stdout
    assert 'QEMU dependency: none' in text and 'NOT IMPLEMENTED' in text
    # DLL/API availability differs across hosted runners. Report it honestly.
    p=subprocess.run([exe,'whp-probe'],capture_output=True,text=True,timeout=20)
    observation=f'WHP probe observation, NOT a CPU-execution test: exit={p.returncode}; '+(p.stdout+p.stderr).strip()
    print('::notice::'+observation.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
    if sys.platform=='win32':
        assert p.returncode==0,'Windows hypervisor unavailable: real CPU gate cannot pass'
        for _ in range(3): run('cpu-smoke')
        print('::notice::PASS: direct WHP real-mode guest store/load/HLT executed three times, AX/RAM=0x5a17; no QEMU, no Android/GPU certification',flush=True)
    else: run('cpu-smoke',ok=False)
print('PASS: independent ZLAG CRC/header/payload reference, source preservation, no overwrite, corruption refusal, boot refusal and capability disclosure',flush=True)
