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
    p=subprocess.run([exe,*map(str,args)],capture_output=True,text=True)
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
    p=subprocess.run([exe,'whp-probe'],capture_output=True,text=True)
    print('WHP observation (not CPU execution certification):',p.returncode,p.stdout,p.stderr,flush=True)
print('PASS: independent ZLAG CRC/header/payload reference, source preservation, no overwrite, corruption refusal, boot refusal and capability disclosure',flush=True)
