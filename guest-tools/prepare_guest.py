"""Build an experimental UEFI Android-x86 disk. NOT an Android source build or certification."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def payloads(tree):
    required = ['kernel', 'initrd.img']
    if not all((tree / name).is_file() for name in required):
        raise ValueError('Extracted ISO must contain root kernel and initrd.img')
    if not any((tree / name).is_file() for name in ['system.sfs', 'system.img', 'system.efs']):
        raise ValueError('Unsupported ISO: no root system.sfs/system.img/system.efs. Root files: ' + ', '.join(sorted(p.name for p in tree.iterdir())[:60]))
    names = sorted(p.name for p in tree.iterdir() if p.is_file() and (p.name in required or p.suffix in ['.sfs', '.img', '.efs']))
    for name in names:
        if (tree / name).is_symlink():
            raise ValueError('Payload symlinks are not accepted')
    return names


def grub_config(profile):
    options = {'no-avx': '', 'drm-gbm': 'HWC=drm GRALLOC=gbm', 'drm-gbm-no-avx': 'HWC=drm GRALLOC=gbm'}[profile]
    return '''set timeout=0
menuentry 'Nexus experimental Android candidate' {
 search --no-floppy --label NEXUS_SYS --set=root
 linux /android/kernel root=/dev/ram0 SRC=/android %s
 initrd /android/initrd.img
}
''' % options


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def build(args):
    # Import only here so recipe/guard tests run without privileged image tooling.
    import os
    os.environ['LIBGUESTFS_BACKEND'] = 'direct'
    os.environ['LIBGUESTFS_BACKEND_SETTINGS'] = 'force_tcg'
    import guestfs
    tree = Path(args.tree).resolve()
    output = Path(args.output).resolve()
    manifest = output.with_suffix('.json')
    if output.exists() or manifest.exists():
        raise FileExistsError('Image or manifest exists; nothing will be overwritten')
    iso = Path(args.iso).resolve()
    if digest(iso) != args.sha256.lower():
        raise ValueError('ISO fingerprint mismatch')
    names = payloads(tree)
    if sum((tree / name).stat().st_size for name in names) > 5 * 1024**3:
        raise ValueError('Payload exceeds supported candidate disk budget')
    for program in ['qemu-img', 'grub-mkstandalone']:
        if not shutil.which(program):
            raise RuntimeError('Missing required tool: ' + program)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='nexus-guest-', dir=output.parent) as folder:
        work = Path(folder)
        config = work / 'grub.cfg'
        config.write_text(grub_config(args.profile))
        efi = work / 'BOOTX64.EFI'
        subprocess.run(['grub-mkstandalone', '-O', 'x86_64-efi', '--modules=part_gpt fat ext2 normal linux search search_label', '-o', str(efi), 'boot/grub/grub.cfg=' + str(config)], check=True)
        disk = work / 'candidate.qcow2'
        subprocess.run(['qemu-img', 'create', '-f', 'qcow2', str(disk), '8G'], check=True)
        g = guestfs.GuestFS(python_return_dict=True)
        try:
            # The only attached disk is the newly created virtual file above.
            g.add_drive_opts(str(disk), format='qcow2', readonly=False)
            g.launch()
            g.part_init('/dev/sda', 'gpt')
            g.part_add('/dev/sda', 'p', 2048, 526335)
            g.part_add('/dev/sda', 'p', 526336, -2048)
            g.part_set_gpt_type('/dev/sda', 1, 'C12A7328-F81F-11D2-BA4B-00A0C93EC93B')
            g.mkfs('vfat', '/dev/sda1')
            g.mkfs('ext4', '/dev/sda2')
            g.set_label('/dev/sda2', 'NEXUS_SYS')
            g.mount('/dev/sda2', '/')
            g.mkdir('/android')
            g.mkdir('/android/data')
            if getattr(args, 'native_bridge', False):
                g.write('/android/data/.nexus-provision', b'clean-ci-template-only\n')
                key = getattr(args, 'provision_adb_key', '')
                if not key:
                    raise ValueError('Native CI provisioning requires an ephemeral authorized public key')
                g.write('/android/data/.nexus-provision-adb.pub', Path(key).read_bytes())
            for name in names:
                g.upload(str(tree / name), '/android/' + name)
            g.mkdir_p('/boot/efi')
            g.mount('/dev/sda1', '/boot/efi')
            g.mkdir_p('/boot/efi/EFI/BOOT')
            g.upload(str(efi), '/boot/efi/EFI/BOOT/BOOTX64.EFI')
            g.sync()
            g.umount_all()
            g.shutdown()
        finally:
            g.close()
        subprocess.run(['qemu-img', 'check', str(disk)], check=True)
        compressed = work / 'standalone.qcow2'
        subprocess.run(['qemu-img', 'convert', '-c', '-f', 'qcow2', '-O', 'qcow2', str(disk), str(compressed)], check=True)
        subprocess.run(['qemu-img', 'check', str(compressed)], check=True)
        metadata = {
            'schema': 1, 'status': 'experimental-unverified', 'ready_for_player': False,
            'source_iso_sha256': args.sha256.lower(), 'image_sha256': digest(compressed),
            'format': 'qcow2', 'standalone': True, 'firmware': 'uefi-x86_64',
            'machine': 'q35', 'graphics': 'virtio', 'profile': args.profile,
            'cpu': 'max' if args.profile == 'drm-gbm' else 'max,avx=off,avx2=off,f16c=off,fma=off',
            'kernel_config': config.read_text(), 'payloads': names,
            'note': 'Upstream Android payload, not a new OS. Desktop, persistence, Windows compatibility and distribution rights require validation.'}
        # Exclusive creation prevents overwriting a disk created concurrently.
        with output.open('xb') as destination, compressed.open('rb') as source:
            shutil.copyfileobj(source, destination)
        with manifest.open('x') as stream:
            json.dump(metadata, stream, indent=2)
    print('Experimental image created; not certified or automatically published:', output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--tree', required=True)
    parser.add_argument('--iso', required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--profile', choices=['no-avx', 'drm-gbm', 'drm-gbm-no-avx'], required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--native-bridge', action='store_true')
    parser.add_argument('--provision-adb-key', default='')
    build(parser.parse_args())
