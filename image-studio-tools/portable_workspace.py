"""Runs ONLY inside the bundled Linux maintenance VM, never on Windows.
The host attaches only a newly created image copy as /dev/vda and a disposable
scratch disk as /dev/vdb. No host disks, shares or network are exposed writable.
"""
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess


def run(*arguments):
    return subprocess.run(list(arguments),check=True,capture_output=True,text=True).stdout


def safe_path(root, relative):
    root=Path(root).resolve()
    candidate=root/str(relative).lstrip('/')
    # Refuse symlink traversal rather than resolve absolute Android links into
    # the maintenance OS. Use the real /system path for system-as-root aliases.
    cursor=root
    for part in candidate.relative_to(root).parts:
        if part in ('.','..'): raise ValueError('Path traversal is not allowed')
        cursor=cursor/part
        if cursor.is_symlink(): raise ValueError('Symlink traversal is disabled; select the physical guest path')
    if not candidate.resolve().is_relative_to(root): raise ValueError('Path escapes guest filesystem')
    return candidate


def replace_file(source, target, new_system_file=False):
    source,target=Path(source),Path(target)
    if not source.is_file() or source.is_symlink(): raise ValueError('Choose a regular input file')
    if target.is_symlink() or (target.exists() and not target.is_file()): raise ValueError('Cannot overwrite a symlink/directory')
    metadata=target.stat() if target.exists() else None
    attrs={name:os.getxattr(target,name) for name in os.listxattr(target)} if metadata else {}
    pending=target.with_name(target.name+'.nexus-new')
    with source.open('rb') as inp,pending.open('xb') as out:
        shutil.copyfileobj(inp,out,1024*1024); out.flush(); os.fsync(out.fileno())
    if metadata:
        os.chmod(pending,stat.S_IMODE(metadata.st_mode)); os.chown(pending,metadata.st_uid,metadata.st_gid)
        for name,value in attrs.items(): os.setxattr(pending,name,value)
    else:
        os.chmod(pending,0o644); os.chown(pending,0,0)
        if new_system_file: os.setxattr(pending,'security.selinux',b'u:object_r:system_file:s0')
    os.replace(pending,target)


class PortableWorkspace:
    def __init__(self, root='/mnt/nexus-disk', scratch='/mnt/nexus-work', system='/mnt/nexus-system'):
        self.root=Path(root); self.scratch=Path(scratch); self.system=Path(system)
        self.base=None; self.system_payload=None; self.compression=None
        self.dirty=False; self.mounts=[]

    def mount(self):
        # Only the editor's fixed scratch device may be formatted, never /dev/vda.
        if run('blkid','-o','value','-s','TYPE','/dev/vdb').strip()!='ext4':
            raise ValueError('Scratch disk must have been initialized by the boot service')
        self.root.mkdir(parents=True,exist_ok=True); self.scratch.mkdir(parents=True,exist_ok=True)
        run('mount','-o','nodev,nosuid,noexec','/dev/vdb',str(self.scratch)); self.mounts.append(self.scratch)
        devices=run('lsblk','-nrpo','NAME,FSTYPE','/dev/vda').splitlines()
        for entry in devices:
            fields=entry.split()
            if len(fields)==2 and fields[1]=='ext4' and re.fullmatch(r'/dev/vda(?:[1-9][0-9]*)?',fields[0]):
                run('mount','-o','nodev,nosuid,noexec',fields[0],str(self.root)); self.mounts.append(self.root)
                if (self.root/'android/initrd.img').is_file():
                    self.base=self.root/'android'; return
                if (self.root/'system/etc/init').is_dir() or (self.root/'etc/init').is_dir():
                    self.base=self.root; return
                run('umount',str(self.root)); self.mounts.remove(self.root)
        raise ValueError('No supported Android-x86 ext4 layout. Encrypted/AVB/dynamic-super/EROFS images are not writable here.')

    def open_system(self):
        if not self.base: raise ValueError('Open a working disk first')
        if self.system in self.mounts: return self.system
        source=self.base/'system.sfs'
        raw=self.base/'system.img'
        if source.is_file():
            info=run('unsquashfs','-s',str(source))
            match=re.search(r'(?m)^Compression\s+(\w+)',info)
            if not match or match[1] not in ('gzip','xz','lz4','lzo','zstd'): raise ValueError('Unsupported squashfs compression')
            self.compression=match[1]
            tree=self.scratch/'system-tree'
            if tree.exists():
                if self.system_payload != source: raise ValueError('Retained scratch tree exists; do not overwrite an unfinished edit')
            else:
                run('unsquashfs','-d',str(tree),str(source))
            raw=tree/'system.img'
            self.system_payload=source
        if raw.is_file():
            if run('blkid','-o','value','-s','TYPE',str(raw)).strip()!='ext4': raise ValueError('Nested system must be ext4; EROFS/AVB rewriting is not supported')
            self.system.mkdir(parents=True,exist_ok=True)
            run('mount','-o','loop,nodev,nosuid,noexec',str(raw),str(self.system)); self.mounts.append(self.system)
            return self.system
        if (self.base/'system/etc/init').is_dir(): return self.base
        raise ValueError('No recognized system.sfs → system.img, raw system.img or system-as-root layout')

    def add_apk(self, source):
        source=Path(source)
        if source.suffix.lower()!='.apk': raise ValueError('Choose an APK')
        info=run('aapt','dump','badging',str(source))
        package=re.search(r"^package: name='([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+)'",info)
        if not package: raise ValueError('Could not validate APK package metadata')
        root=self.open_system()
        prefix='system' if (root/'system/app').is_dir() else ''
        apps=safe_path(root,Path(prefix)/'app')
        if not apps.is_dir(): raise ValueError('Recognized system app directory missing')
        folder=safe_path(root,apps.relative_to(root)/('NexusAdded-'+package[1]))
        if folder.exists(): raise FileExistsError('Package staging folder already exists; use file replacement explicitly')
        folder.mkdir(mode=0o755); os.chown(folder,0,0)
        os.setxattr(folder,'security.selinux',b'u:object_r:system_file:s0')
        replace_file(source,folder/'base.apk',new_system_file=True)
        self.dirty=True
        return package[1]

    def save(self):
        if self.system in self.mounts:
            run('sync'); run('umount',str(self.system)); self.mounts.remove(self.system)
        if self.system_payload and self.dirty:
            output=self.scratch/'nexus-system.sfs'
            if output.exists(): raise FileExistsError('Previous repack output retained; inspect before retry')
            run('mksquashfs',str(self.scratch/'system-tree'),str(output),'-noappend','-comp',self.compression,'-processors','2')
            replace_file(output,self.system_payload)
            output.unlink()
        run('sync')
        self.dirty=False

    def close(self):
        if self.dirty: raise ValueError('Save changes before shutting down; scratch files have been retained')
        for mount in list(reversed(self.mounts)):
            run('umount',str(mount)); self.mounts.remove(mount)
        run('sync')
