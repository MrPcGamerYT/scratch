NEXUS PORTABLE GUEST EDITOR - WINDOWS 10 X64 PREVIEW

Extract the ENTIRE ZIP to a writable folder. Run Start-Nexus-Editor.cmd.
Your default browser opens the local interface. Keep the console open.
No WSL, Docker, installed Python, disk driver or administrator rights needed.
The bundled maintenance OS runs under software emulation (TCG).
Requirements: x64 Windows 10, about 2 GiB spare RAM, modern browser, and free
space equal to the guest virtual size + 4 GiB (more if system.sfs is large).

1. Fully shut down Nexus Player and all VMs using the source image.
2. Choose the source and a NEW destination ending in .qcow2.
3. Create copy & open. The original is never attached to the maintenance VM.
4. Browse 'System contents' to edit system APKs; browse 'Android disk folder'
   for initrd/kernel/data files. Nested squashfs is unpacked automatically.
5. Select a local file, select/type the target guest path, then import/replace.
   Add APK stages a new system app without deleting the existing launcher.
6. Save & Finish. Only then boot the NEW copy in an isolated player instance.

LIMITATIONS / RECOVERY
- Preview, not Android compatibility certification. Replacing system APKs can
  break boot, signatures, native-library compatibility or privileged permissions.
- Existing launcher is kept by Add APK. Select the new Home activity after boot.
- Export/import at most 64 MiB per file. No split APK install support.
- Recognizes ext4 Android-x86 and system.sfs containing ext4 system.img.
  Encrypted, EROFS, dynamic-super and verified boot layouts are not supported.
- No directory deletion and no following guest symlinks.
- New system files get generic system_file SELinux context, not privileged grants.
- Do not replace system.img/system.sfs directly while System contents is mounted.
- If an error occurs, retain both the new .qcow2 and its .nexus-work folder/log.
  Never boot that copy while the maintenance VM is active. Closing the console
  without Save & Finish may leave changes unrepacked. Original remains safe.
- Use Nexus early boot banner replaces only the recognized initrd ASCII banner.
  Unknown banners are refused. Copyright/licence notices and package IDs remain.

This separate portable utility does not replace or remove the existing Image
Studio ISO installer or the developer Linux/WSLg file editor.

THIRD PARTY
Python licence: python/LICENSE.txt. QEMU licences: qemu/licenses.
Bundled Linux package list: appliance/PACKAGES.tsv.
Bundled package copyright/licence files: appliance/LICENCES.tar.gz.
Ubuntu package source: https://archive.ubuntu.com/ubuntu/pool/ (versions listed
in PACKAGES.tsv); Linux and QEMU components remain under their original licences.
