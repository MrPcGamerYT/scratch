"""Inspect pinned boot contract without executing or rewriting upstream init."""
import gzip
import io
import json
from pathlib import Path
import stat

LIMIT=64*1024*1024


def archive(data):
    if data.startswith(b'\x1f\x8b'):
        with gzip.GzipFile(fileobj=io.BytesIO(data)) as source:
            data=source.read(LIMIT+1)
    if len(data)>LIMIT:
        raise ValueError('Ramdisk expands beyond inspection budget')
    at=0;result={};duplicates=[]
    while at+110<=len(data):
        header=data[at:at+110]
        if header[:6]!=b'070701':
            raise ValueError('Only gzip/plain newc inspection is supported')
        fields=[int(header[6+i*8:14+i*8],16) for i in range(13)]
        size,namesize=fields[6],fields[11]
        if not 1<=namesize<=4096 or at+110+namesize>len(data):
            raise ValueError('Invalid newc name')
        encoded=data[at+110:at+110+namesize]
        if encoded[-1]!=0:
            raise ValueError('Unterminated newc name')
        name=encoded[:-1].decode();body_at=(at+110+namesize+3)&~3;end=body_at+size
        if end>len(data):
            raise ValueError('Truncated newc member')
        if name=='TRAILER!!!':
            return result,duplicates
        key=name.removeprefix('./')
        if key in result:
            duplicates.append(key)
        result[key]=(fields[1],data[body_at:end]);at=(end+3)&~3
    raise ValueError('No newc trailer')


def inspect(root):
    root=Path(root)
    config=(root/'kernel.config').read_text() if (root/'kernel.config').exists() else ''
    keys=['ANDROID_BINDER_IPC','ANDROID_BINDERFS','ANDROID_BINDER_DEVICES','ASHMEM',
          'VIRTIO','VIRTIO_MMIO','VIRTIO_MMIO_CMDLINE_DEVICES','VIRTIO_BLK','VIRTIO_CONSOLE',
          'EXT4_FS','SQUASHFS','BLK_DEV_LOOP','FB','FB_SIMPLE','FB_VESA','SYSFB_SIMPLEFB']
    values={line.split('=',1)[0][7:]:line.split('=',1)[1] for line in config.splitlines() if line.startswith('CONFIG_') and '=' in line}
    result={'driver_configuration':{key:values.get(key,'not set' if config else 'unknown') for key in keys},
            'ramdisks':{},'android_init_executed':False,'android_home_verified':False}
    for name in ['initrd.img','ramdisk.img']:
        path=root/name
        if not path.exists():
            continue
        try:
            items,duplicates=archive(path.read_bytes())
            scripts={key:body[:128*1024].decode(errors='replace') for key,(mode,body) in items.items()
                     if key in ['init','init.rc','fstab.android_x86','fstab.android_x86_64','scripts/0-auto-detect','scripts/2-mount'] and stat.S_ISREG(mode)}
            result['ramdisks'][name]={'entries':list(items)[:200],'duplicate_names':duplicates[:100],'inspection_policy':'last record reported; no extraction or mutation','boot_scripts':scripts}
        except ValueError as error:
            result['ramdisks'][name]={'inspection_unavailable':str(error)}
    return result


if __name__=='__main__':
    root=Path('android-boot-assets');report=inspect(root)
    Path('android-boot-contract.json').write_text(json.dumps(report,indent=2))
    print('::notice::ANDROID_DRIVER_CONTRACT='+json.dumps(report['driver_configuration']),flush=True)
    for name,data in report['ramdisks'].items():
        for script,body in data.get('boot_scripts',{}).items():
            excerpt=body[-9000:] if script=='init' else body[:6000]
            for at in range(0,len(excerpt),3000):
                text=f'ANDROID_INIT_SCRIPT {name}/{script} chunk {at}: '+excerpt[at:at+3000]
                print('::notice::'+text.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
