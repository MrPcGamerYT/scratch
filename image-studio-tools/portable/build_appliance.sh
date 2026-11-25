#!/bin/bash
set -euo pipefail
# Build an initramfs appliance from Ubuntu's signed binary packages. No full OS
# install/chroot bootstrap, no bootloader, no persistent appliance root disk.
sudo apt-get install -y --no-install-recommends linux-image-virtual aapt busybox-static cpio kmod
root="$PWD/appliance-root"
mkdir -p "$root"/{bin,sbin,usr/bin,usr/sbin,etc,proc,sys,dev,tmp,run,mnt,opt/nexus,lib}
copy_binary() {
  local binary="$1"
  cp -L --parents "$binary" "$root"
  while read -r lib; do
    [ -f "$lib" ] && cp -L --parents "$lib" "$root"
  done < <(ldd "$binary" 2>/dev/null | grep -oE '/[^ ]+' | grep -v '^(0x' || true)
}
for cmd in python3 mount umount blkid lsblk mkfs.ext4 sync unsquashfs mksquashfs aapt modprobe; do
  binary="$(readlink -f "$(command -v "$cmd")")"
  copy_binary "$binary"
  mkdir -p "$root$(dirname "$(command -v "$cmd")")"
  cp -L "$binary" "$root$(command -v "$cmd")"
done
cp -L /bin/busybox "$root/bin/busybox"
for cmd in sh ip sleep mkdir cat; do ln -sf busybox "$root/bin/$cmd"; done
# Python stdlib and extension dependencies, but no third-party host site-packages.
stdlib="$(python3 -c 'import sysconfig; print(sysconfig.get_path("stdlib"))')"
mkdir -p "$root$(dirname "$stdlib")"
cp -a "$stdlib" "$root$(dirname "$stdlib")/"
rm -rf "$root$stdlib"/{test,tests,ensurepip,idlelib,tkinter,__pycache__}
find "$root$stdlib" -name '*.pyc' -delete
while IFS= read -r file; do
  while read -r lib; do [ -f "$lib" ] && cp -L --parents "$lib" "$root"; done < <(ldd "$file" 2>/dev/null | grep -oE '/[^ ]+' || true)
done < <(find "$root$stdlib" -name '*.so')
kernel="$(ls /boot/vmlinuz-*-generic | sort -V | tail -1)"
version="${kernel##*/vmlinuz-}"
mkdir -p "$root/lib/modules"
cp -a "/lib/modules/$version" "$root/lib/modules/"
# On merged-/usr builds modules may contain build/source links: never follow them.
rm -f "$root/lib/modules/$version/build" "$root/lib/modules/$version/source"
cp image-studio-tools/portable_workspace.py image-studio-tools/portable/appliance_server.py "$root/opt/nexus/"
cat > "$root/init" <<'EOF'
#!/bin/sh
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
mount -t tmpfs tmpfs /run
mount -t tmpfs tmpfs /tmp
modprobe virtio_pci
modprobe virtio_blk
modprobe virtio_net
modprobe ext4
modprobe loop
ip link set lo up
ip link set eth0 up
ip addr add 10.0.2.15/24 dev eth0
mkdir -p /mnt/nexus-disk /mnt/nexus-work /mnt/nexus-system
exec /usr/bin/python3 /opt/nexus/appliance_server.py
EOF
chmod +x "$root/init"
# Catch missing loaders/shared libraries/stdlib before spending time booting.
sudo chroot "$root" /usr/bin/python3 -c 'import base64,http.server,json,subprocess,pathlib,gzip; print("Appliance Python OK")'
mkdir -p appliance
sudo cp "$kernel" appliance/vmlinuz
(cd "$root" && find . -print0 | cpio --null -o --format=newc 2>/dev/null | gzip -1) > appliance/initrd
# Build-host package inventory is a superset of bundled runtime dependencies.
dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' > appliance/PACKAGES.tsv
find /usr/share/doc -name copyright -print0 | tar --null -T - -czf appliance/LICENCES.tar.gz 2>/dev/null
sudo chown -R "$(id -u):$(id -g)" appliance
