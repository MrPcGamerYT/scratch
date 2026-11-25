"""Package a pristine experimental candidate only after the boot gate succeeds."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def sha256(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def package(candidate, output, firmware, variables, notices):
    candidate, output = Path(candidate), Path(output)
    result = json.loads((candidate / 'evidence/result.json').read_text())
    metadata = json.loads((candidate / 'android-candidate.json').read_text())
    if result.get('status') != 'framework_ready_visual_unverified' or not result.get('framework_boot_completed'):
        raise ValueError('Android boot gate did not pass; refusing to publish a disk')
    disk = candidate / 'android-candidate.qcow2'
    if sha256(disk) != metadata.get('image_sha256'):
        raise ValueError('Pristine disk differs from builder hash; verification must use an overlay')
    if output.exists():
        raise FileExistsError('Package output exists')
    required = [Path(firmware), Path(variables), Path(notices), Path(__file__).with_name('Start-ExperimentalGuest.ps1')]
    if not all(path.is_file() and path.stat().st_size for path in required):
        raise ValueError('Firmware, notices or launcher missing')
    output.mkdir(parents=True)
    # Same filesystem in CI: retain the immutable image without another multi-GB copy.
    try:
        (output / disk.name).hardlink_to(disk)
    except OSError:
        shutil.copyfile(disk, output / disk.name)
    for source, name in [(required[0], 'uefi-code.fd'), (required[1], 'uefi-vars-template.fd'),
                         (required[2], 'OVMF-NOTICES.txt'), (required[3], required[3].name),
                         (candidate / 'android-candidate.json', 'compatibility.json'),
                         (candidate / 'evidence/result.json', 'linux-boot-result.json')]:
        shutil.copyfile(source, output / name)
    (output / 'README.txt').write_text('''NEXUS EXPERIMENTAL ANDROID GUEST - NOT A FINISHED PRODUCT

This is a prepared upstream Bliss OS 16.9.7 Android 13 GApps diagnostic image,
not an Android OS written by Nexus and not a proprietary replacement for Android.
Upstream source/download and notices: https://sourceforge.net/projects/blissos-x86/
https://github.com/BlissOS  https://blissos.org/
Upstream component and Google software terms remain applicable. This package is
not a grant of redistribution rights or Google Play certification.

Verified in this build: Linux QEMU TCG disk-only UEFI Android service readiness.
Earlier matching recipe: readable Hi there Setup Wizard screen inspected in CI.
Not verified: this exact build's screen until evidence review, Windows guest boot,
setup completion, second boot/persistence, audio, networking, games, acceleration.
Mesa warnings may remain. Do not call this a ready-to-use or fastest emulator.

Extract the complete GitHub artifact to one permanent folder. Verify SHA256SUMS.txt
before use. Do NOT rename/copy this into an existing Player instance or replace
an existing backing disk. Current Player defaults do not apply every tested CPU
setting; this is not an automatic Player image update.

On Windows run Start-ExperimentalGuest.ps1 with PowerShell and select
qemu-system-x86_64.exe from your fully extracted Nexus Developer package.
The launcher uses TCG, 2 CPUs, 3072 MB, VirtIO VGA, supplied UEFI firmware and
reduced AVX/FMA exposure. Guest compositor settings are in the disk bootloader.
It creates a NEW retained instance under LOCALAPPDATA/ProjectNexus/experimental
each time; it is a first-boot diagnostic launcher, not a persistent-instance UI.
It does not add ADB port forwarding. This does not prove ADB is disabled inside
the upstream guest; treat the image as a development guest, not for sensitive data.
It does not configure audio. Exit by clean Android shutdown where possible.
Keep the base in place while its overlays are in use. No host physical disks
are attached. Do not share the per-user overlay or firmware state.

Hashes cover the pristine downloadable base and template, not later guest writes.
GitHub artifact links may require signing in and expire with artifact retention.
''')
    files = sorted(path for path in output.iterdir() if path.is_file())
    (output / 'SHA256SUMS.txt').write_text(''.join(f'{sha256(path)}  {path.name}\n' for path in files))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--firmware', required=True)
    parser.add_argument('--variables', required=True)
    parser.add_argument('--notices', required=True)
    args = parser.parse_args()
    package(args.candidate, args.output, args.firmware, args.variables, args.notices)
