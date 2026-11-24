"""Create portable ZIP64 downloads; verify every entry and emit SHA-256."""
import argparse
import hashlib
from pathlib import Path
import zipfile


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def build_archive(source, destination, required):
    source, destination = Path(source), Path(destination)
    for name in required:
        if not (source / name).is_file():
            raise ValueError(f'Package dependency missing: {name}')
    files = sorted(path for path in source.rglob('*') if path.is_file())
    if not files:
        raise ValueError('Refusing empty package')
    manifest = {path.relative_to(source).as_posix(): sha256_file(path) for path in files}
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation: a retry cannot accidentally publish a stale archive.
    with zipfile.ZipFile(destination, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=6, allowZip64=True) as archive:
        for path in files:
            archive.write(path, path.relative_to(source).as_posix())
        archive.writestr('MANIFEST.sha256', ''.join(f'{digest}  {name}\n' for name, digest in manifest.items()))
    with zipfile.ZipFile(destination) as archive:
        corrupt = archive.testzip()
        if corrupt:
            raise ValueError(f'ZIP CRC failure: {corrupt}')
        for name, expected in manifest.items():
            digest = hashlib.sha256()
            with archive.open(name) as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(chunk)
            if digest.hexdigest() != expected:
                raise ValueError(f'ZIP content verification failed: {name}')
    checksum = sha256_file(destination)
    destination.with_suffix(destination.suffix + '.sha256').write_text(f'{checksum}  {destination.name}\n', encoding='ascii')
    print(f'VERIFIED: {destination.name} | {destination.stat().st_size} bytes | SHA256 {checksum}', flush=True)
    return checksum


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('player', type=Path)
    parser.add_argument('studio', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    common = ['Qt6Core.dll', 'Qt6Widgets.dll', 'platforms/qwindows.dll', 'qemu/bin/qemu-system-x86_64.exe', 'qemu/bin/qemu-img.exe']
    build_archive(args.player, args.output / 'player/ProjectNexus-Windows.zip', common + ['ProjectNexus.exe'])
    build_archive(args.studio, args.output / 'studio/ProjectNexus-ImageStudio-Developer.zip', common + ['NexusImageStudio.exe', 'image-studio.ps1', 'prepare-android.ps1', 'launch.ps1', 'offline-files/offline_editor.py', 'qemu/share/qemu/edk2-x86_64-code.fd', 'qemu/share/qemu/edk2-i386-vars.fd'])
