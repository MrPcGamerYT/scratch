"""Real QEMU/TCG boot and nested squashfs/ext4 round trip; no Android claim."""
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

spec=importlib.util.spec_from_file_location('host','image-studio-tools/portable/editor_host.py')
h=importlib.util.module_from_spec(spec); spec.loader.exec_module(h)
def run(*args): return subprocess.run([str(a) for a in args],check=True,capture_output=True,text=True).stdout
def digest(p): return hashlib.sha256(p.read_bytes()).hexdigest()
with tempfile.TemporaryDirectory() as d:
    root=Path(d); dist=root/'distribution'; dist.mkdir()
    (dist/'appliance').symlink_to(Path('appliance').resolve(),target_is_directory=True)
    (dist/'qemu/bin').mkdir(parents=True); (dist/'qemu/share').mkdir()
    for name in ['qemu-system-x86_64','qemu-img']: (dist/'qemu/bin'/name).symlink_to(shutil.which(name))
    data=dist/'qemu/share/qemu'; data.mkdir()
    for folder in ['/usr/share/qemu','/usr/share/seabios','/usr/share/ipxe/qemu']:
        if Path(folder).is_dir():
            for file in Path(folder).iterdir():
                if not (data/file.name).exists(): (data/file.name).symlink_to(file)
    h.ROOT=dist
    tree=root/'sys'; (tree/'system/app/OldLauncher').mkdir(parents=True)
    old=tree/'system/app/OldLauncher/base.apk'; old.write_bytes(b'old-launcher'); old.chmod(0o640)
    system=root/'nested'; system.mkdir(); raw=system/'system.img'
    with raw.open('wb') as f:f.truncate(128*1024**2)
    run('mkfs.ext4','-F','-d',tree,raw)
    outer=root/'outer'; (outer/'android').mkdir(parents=True)
    (outer/'android/initrd.img').write_bytes(b'fixture-not-Android')
    run('mksquashfs',system,outer/'android/system.sfs','-noappend','-comp','xz','-processors','2')
    source=root/'source.img'
    with source.open('wb') as f:f.truncate(512*1024**2)
    run('mkfs.ext4','-F','-d',outer,source)
    run('qemu-img','convert','-f','raw','-O','qcow2',source,Path('appliance/test-fixture.qcow2'))
    before=digest(source)
    try:
        print(h.open_copy(source,root/'edited.qcow2'),flush=True)
        result=h.guest({'op':'list','view':'system','path':'/system/app/OldLauncher'})
        assert result['entries'][0]['name']=='base.apk',result
        for path in ['/../../etc/passwd','/system/../../../etc/passwd']:
            try:h.guest({'op':'export','view':'system','path':path})
            except ValueError:pass
            else:raise AssertionError('Traversal accepted')
        h.guest({'op':'import','view':'system','path':'/system/app/OldLauncher/base.apk','data':base64.b64encode(b'replacement').decode()})
        assert base64.b64decode(h.guest({'op':'export','view':'system','path':'/system/app/OldLauncher/base.apk'})['data'])==b'replacement'
        h.guest({'op':'save'}); h.guest({'op':'close'})
    finally:
        if h.vm and h.vm.poll() is None:h.vm.terminate();h.vm.wait(timeout=30)
        if h.log:h.log.close()
        logs=list(root.glob('*.nexus-work/maintenance.log'))
        for log in logs:
            Path('appliance-test.log').write_text(log.read_text(errors='replace'))
    assert digest(source)==before,'Original changed!'
    extracted=root/'edited.raw';run('qemu-img','convert','-O','raw',root/'edited.qcow2',extracted)
    sfs=root/'edited.sfs';run('debugfs','-R',f'dump /android/system.sfs {sfs}',extracted)
    unpack=root/'unpack';run('unsquashfs','-d',unpack,sfs)
    payload=root/'payload';run('debugfs','-R',f'dump /system/app/OldLauncher/base.apk {payload}',unpack/'system.img')
    assert payload.read_bytes()==b'replacement'
    info=run('debugfs','-R','stat /system/app/OldLauncher/base.apk',unpack/'system.img')
    assert '0640' in info,info
    print('PASS: real TCG maintenance boot; nested squashfs/ext4 replacement; preserved mode; traversal refusal; source SHA unchanged; saved image independently extracted',flush=True)
