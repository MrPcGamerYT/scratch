"""Patch the recognized Android-x86 ramdisk location, preserving system fs metadata.
Run only on a freshly extracted ISO tree, never a user's installed disk.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from install_bridge_ramdisk import patch, record, records
import stat


def install(tree, agent, evidence):
    tree,agent,evidence=Path(tree),Path(agent),Path(evidence)
    evidence.mkdir(parents=True,exist_ok=True)
    if (tree/'ramdisk.img').is_file():
        source=tree/'ramdisk.img'; original=source.read_bytes()
        (evidence/'upstream-ramdisk.img').write_bytes(original)
        source.write_bytes(patch(original,agent.read_bytes()))
        return
    system=tree/'system.sfs'
    if not system.is_file(): raise ValueError('No recognized separate ramdisk or system.sfs')
    os.environ['LIBGUESTFS_BACKEND']='direct'
    os.environ['LIBGUESTFS_BACKEND_SETTINGS']='force_tcg'
    import guestfs
    with tempfile.TemporaryDirectory(prefix='nexus-system-',dir=tree.parent) as temp:
        work=Path(temp)
        info=subprocess.run(['unsquashfs','-s',str(system)],capture_output=True,text=True,check=True).stdout
        (evidence/'squashfs-layout.txt').write_text(info)
        match=re.search(r'(?m)^Compression\s+(\w+)',info)
        compression=match[1] if match else ''
        if compression not in ['gzip','lz4','xz','zstd','lzo']:
            raise ValueError('Unsupported source squashfs compression')
        subprocess.run(['unsquashfs','-d',str(work/'sfs'),str(system)],check=True)
        image=work/'sfs/system.img'
        if not image.is_file(): raise ValueError('system.sfs does not contain a standalone system.img')
        g=guestfs.GuestFS(python_return_dict=True)
        try:
            g.add_drive_opts(str(image),format='raw',readonly=False)
            g.launch(); g.mount('/dev/sda','/')
            layout={'root':g.ls('/'),'nested_system':g.ls('/system') if g.is_dir('/system') else []}
            (evidence/'system-image-layout.json').write_text(json.dumps(layout,indent=2))
            print('::notice title=Android 11 system layout::'+json.dumps(layout))
            # system-as-root may also expose /etc as a symlink to /system/etc.
            init_roots=[prefix for prefix in ['/system', ''] if g.is_file(prefix+'/etc/init/hw/init.rc') and g.is_dir(prefix+'/etc/init')][:1]
            if len(init_roots)==1:
                prefix=init_roots[0]
                # Android 11 init automatically imports /system/etc/init/*.rc.
                # Reuse the exact reviewed payloads from the ramdisk recipe.
                seed=record('init.rc',[0,stat.S_IFREG|0o644,0,0,1,0,0,0,0,0,0,0,0],b'')+record('TRAILER!!!',[0]*13,b'')
                for name,fields,body in records(patch(seed,agent.read_bytes())):
                    if name=='init.rc': continue
                    if name=='init.nexus.rc':
                        target=prefix+'/etc/init/nexus.rc'
                        body=body.replace(b'/nexus-agent',b'/system/bin/nexus-agent').replace(b'/nexus-provision.sh',b'/system/bin/nexus-provision.sh')
                    else: target=prefix+'/bin/'+name
                    if g.exists(target): raise ValueError('Existing bridge file: '+target)
                    g.write(target,body); g.chown(0,0,target); g.chmod(fields[1]&0o7777,target)
                    label='u:object_r:system_file:s0'
                    g.setxattr('security.selinux',label,len(label),target)
                (evidence/'integration-method.txt').write_text('Recognized Android 11 system init directory; SELinux system_file labels retained for new files.\n')
            else:
                candidates=[path for path in ['/ramdisk.img','/system/ramdisk.img'] if g.is_file(path)]
                if len(candidates)!=1: raise ValueError('No unique recognized system init directory or nested ramdisk; refusing guessed modification')
                target=candidates[0]
                original=work/'original-ramdisk.img'; g.download(target,str(original))
                shutil.copyfile(original,evidence/'upstream-ramdisk.img')
                modified=work/'nexus-ramdisk.img'; modified.write_bytes(patch(original.read_bytes(),agent.read_bytes()))
                metadata=g.statns(target)
                g.upload(str(modified),target)
                g.chown(metadata['st_uid'],metadata['st_gid'],target)
                g.chmod(metadata['st_mode'] & 0o7777,target)
            g.sync(); g.umount_all(); g.shutdown()
        finally:
            g.close()
        output=work/'nexus-system.sfs'
        subprocess.run(['mksquashfs',str(work/'sfs'),str(output),'-noappend','-comp',compression,'-processors','2'],check=True)
        shutil.copyfile(output,system)


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    for key in ['tree','agent','evidence']: parser.add_argument('--'+key,required=True)
    args=parser.parse_args()
    try: install(args.tree,args.agent,args.evidence)
    except Exception as error:
        print('::error::Native guest integration failed: '+str(error)); raise
