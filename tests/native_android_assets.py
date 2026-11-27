"""Fingerprint copied Android-x86 boot payloads; never mount, modify or execute them."""
import hashlib
import json
from pathlib import Path
import struct
import sys

SOURCE_SHA256='44e4144a3d31f8233a5826b6a5079c4514589286a2f9b59bdf8d303d10e8f577'


def manifest(root):
    root=Path(root)
    files={}
    for name in ['kernel','initrd.img','ramdisk.img']:
        path=root/name
        if not path.exists() and name=='ramdisk.img':
            continue
        if path.is_symlink() or not path.is_file() or not 0<path.stat().st_size<=64*1024*1024:
            raise ValueError('Missing/invalid/oversized copied boot payload: '+name)
        data=path.read_bytes()
        files[name]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
    kernel=(root/'kernel').read_bytes()
    if len(kernel)<0x268 or kernel[0x1fe:0x200]!=b'\x55\xaa' or kernel[0x202:0x206]!=b'HdrS':
        raise ValueError('Source kernel does not contain an x86 bzImage setup header')
    return {'schema':1,'source':'Bliss-v14.10.3-x86_64-OFFICIAL-foss-20241012.iso',
            'source_iso_sha256':SOURCE_SHA256,'fingerprint_not_publisher_signature':True,
            'files':files,'linux_boot_protocol':hex(struct.unpack_from('<H',kernel,0x206)[0]),
            'kernel_changed':False,'android_init_executed':False,'android_home_verified':False,
            'scope':'Unmodified copied upstream boot files. New native engine integration, not legacy QEMU evidence.'}


if __name__=='__main__':
    root=Path(sys.argv[1]);data=manifest(root)
    with (root/'manifest.json').open('x') as file:
        json.dump(data,file,indent=2)
    print(json.dumps(data,indent=2),flush=True)
