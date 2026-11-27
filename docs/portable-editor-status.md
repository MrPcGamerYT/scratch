# Portable Windows 10 guest editor — verified preview

## Download

- [Nexus-Portable-Guest-Editor-Win10.zip](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/releases/download/portable-editor-45678447414/Nexus-Portable-Guest-Editor-Win10.zip)
- Size: 144,998,164 bytes (~138 MiB).
- SHA256: `87f1a3697805e9949708e785951a2ddb26e724cc2b7d2ff0c881b9987502c905`.
- [Build and real VM tests](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45678447414): SUCCESS.

Extract the entire ZIP, run **Start-Nexus-Editor.cmd**, and keep its console open.
The browser interface is local-only. No WSL, Docker, Python installation, disk
driver, administrator access or separately installed command-line tools needed.
A modern browser (such as Edge) is required. This is a separate utility; the
existing Image Studio ISO installer and developer Linux file editor are retained.

## Features and safety

- Bundled Windows Python, QEMU and direct-boot Linux maintenance initramfs.
- Software emulation baseline; hardware virtualization is not required.
- Create a NEW standalone qcow2 working copy from an unencrypted standalone raw
  or qcow2 source. Existing destinations/backing chains are refused. Keep the
  source stopped throughout copying.
- Original disk is never attached to the maintenance VM. Only working copy and
  disposable scratch disk are attached. No writable host shares; restricted
  user-mode networking exposes only an authenticated loopback API.
- Browse/import/export/create directories, replace regular files, stage APKs
  under system/app. Existing launcher is retained when adding an APK.
- Recognizes ext4 Android-x86 layouts and system.sfs containing ext4 system.img.
- Replacements preserve existing mode/owner/xattrs. New system files receive
  generic system_file SELinux context. Symlink traversal is refused.
- Save repacks nested squashfs; Save & Finish unmounts and releases the disk.
- **Use Nexus early boot banner** replaces only the recognized initrd text_art
  banner. Copyright, licences, warnings, properties and package names remain.

## Verified evidence / limits

Six host-side safety/branding unit tests pass. Linux CI booted a real maintenance
VM under TCG, edited a nested squashfs/ext4 fixture, saved/unmounted, independently
extracted the result and checked replacement data, original mode and unchanged
source SHA. Windows CI used the exact bundled embedded Python and Windows QEMU
runtime to boot, import/export, save/unmount the nested fixture and confirm its
source SHA was unchanged. Package creation checked ZIP CRC and per-file SHA.

The banner patch was also run against the actual initrd from verified guest
45648355363: only init content changed, mode/owner metadata and the upstream
copyright/licence prefix stayed intact. [Actual-image check](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45678447480).
This is an offline patch check, NOT a new Android boot/screenshot certification.

This preview is not proof that arbitrary launcher APKs will boot or that Android
will grant requested signature/privileged permissions. The fixture APK payload
used in filesystem tests is not an installable Android app. Select the new Home
activity after a separate Android boot test; retain a working fallback launcher.
No newly branded full Android guest image is published by this tool release.

Import/export is limited to 64 MiB/file. No split-APK installation, deletion,
symlink following, AVB re-signing, EROFS, encrypted or dynamic-super editing.
No automatic early-banner change occurs until its explicit button is pressed.
Allow at least guest virtual size + 4 GiB free disk space and ~2 GiB spare RAM;
large compressed payloads may require additional scratch capacity/time.
On errors retain the new image and `.nexus-work` folder/log for recovery. Never
boot a working image while still open in the editor. The original stays intact.

## Source map

- `image-studio-tools/portable/`: browser UI, bundled-runtime controller, private
  VM API, appliance build script and Windows launcher.
- `image-studio-tools/portable_workspace.py`: Linux filesystem backend.
- `guest-tools/rebrand_boot.py`: bounded initrd banner patch.
- `.github/workflows/portable-editor.yml`: appliance, Linux/Windows gates, release.
- `.github/workflows/verify-portable-download.yml`: independent release download
  SHA/CRC/manifest/extraction verification.

Package licences and the build-host dependency inventory are included. The latter
is a superset of the files actually bundled, not a minimal SBOM.
