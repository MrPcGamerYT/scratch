#!/usr/bin/env bash
# Run inside MSYS2 UCRT64 on the GitHub Windows runner.
set -euo pipefail
root="$(pwd)"
source="$root/qemu-source"
prefix="$root/qemu-bundle"
mkdir -p "$root/qemu-build" "$prefix/share/qemu" "$prefix/sources" "$prefix/licenses"
cd "$root/qemu-build"
"$source/configure" --prefix="$prefix" --target-list=x86_64-softmmu \
  --enable-whpx --enable-sdl --enable-slirp --enable-tools \
  --disable-docs --disable-capstone --disable-install-blobs --disable-werror
ninja -j 2
ninja install
# Only the two firmware images needed by the PC/std-vga configuration are shipped.
cp "$source/pc-bios/bios-256k.bin" "$source/pc-bios/vgabios-stdvga.bin" "$prefix/share/qemu/"
# Resolve runtime dependencies and copy their transitive DLLs next to the executables.
for exe in "$prefix/bin/qemu-system-x86_64.exe" "$prefix/bin/qemu-img.exe"; do
  ldd "$exe" | awk '/\/ucrt64\/bin\/.*\.dll/ {print $3}' | while read -r dll; do cp -n "$dll" "$prefix/bin/"; done
done
# ldd on MinGW prints transitive dependencies. Repeat against every copied DLL.
for dll in "$prefix"/bin/*.dll; do
  ldd "$dll" | awk '/\/ucrt64\/bin\/.*\.dll/ {print $3}' | while read -r dependency; do cp -n "$dependency" "$prefix/bin/"; done
done
cp "$source/COPYING" "$source/LICENSE" "$source/pc-bios/README" "$prefix/licenses/"
cp -r /ucrt64/share/licenses "$prefix/licenses/msys2"
pacman -Q > "$prefix/licenses/msys2-installed-packages.txt"
cp "$root/packaging/THIRD-PARTY.md" "$prefix/licenses/"
# The exact QEMU and SeaBIOS source trees are bundled alongside the binaries.
tar -C "$source" --exclude=.git -czf "$prefix/sources/qemu-10.2.0.tar.gz" .
tar -C "$root/seabios-source" --exclude=.git -czf "$prefix/sources/seabios-1.17.0.tar.gz" .
cp "$root/packaging/build-qemu.sh" "$prefix/sources/"
