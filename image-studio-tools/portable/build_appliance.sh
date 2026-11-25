#!/bin/bash
set -euo pipefail
# CI-only build. All downloaded packages and their licences are retained/listed.
root="$PWD/appliance-root"
sudo debootstrap --variant=minbase --components=main,universe --include=systemd-sysv,linux-image-generic,initramfs-tools,python3,squashfs-tools,e2fsprogs,util-linux,aapt,udev noble "$root" http://archive.ubuntu.com/ubuntu
sudo mkdir -p "$root/opt/nexus" "$root/etc/systemd/system"
sudo cp image-studio-tools/portable_workspace.py image-studio-tools/portable/appliance_server.py "$root/opt/nexus/"
# virtio devices must be available before the real root is mounted.
printf 'virtio_pci\nvirtio_blk\next4\n' | sudo tee -a "$root/etc/initramfs-tools/modules"
sudo chroot "$root" update-initramfs -u -k all
cat <<'EOF' | sudo tee "$root/etc/systemd/system/nexus-editor.service"
[Unit]
Description=Nexus isolated offline editor
After=local-fs.target
[Service]
Type=simple
ExecStart=/usr/bin/python3 /opt/nexus/appliance_server.py
StandardOutput=journal+console
StandardError=journal+console
[Install]
WantedBy=multi-user.target
EOF
sudo chroot "$root" systemctl enable nexus-editor.service
sudo chroot "$root" systemctl disable systemd-networkd-wait-online.service || true
# QEMU's user-mode network has fixed addressing. No external traffic permitted
# by the host's restrict=on option; this config is only for the forwarded API.
cat <<'EOF' | sudo tee "$root/etc/systemd/network/20-nexus.network"
[Match]
Name=en* eth*
[Network]
Address=10.0.2.15/24
EOF
sudo chroot "$root" systemctl enable systemd-networkd.service
sudo rm -rf "$root/var/cache/apt/archives/"*.deb
mkdir -p appliance
sudo cp -L "$root"/boot/vmlinuz-* appliance/vmlinuz
sudo cp -L "$root"/boot/initrd.img-* appliance/initrd
sudo chroot "$root" dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' > appliance/PACKAGES.tsv
sudo tar -C "$root" -czf appliance/LICENCES.tar.gz usr/share/doc
truncate -s 3G appliance/root.raw
sudo mkfs.ext4 -F -L NEXUS_EDITOR -d "$root" appliance/root.raw
sudo chown -R "$(id -u):$(id -g)" appliance
qemu-img convert -f raw -O qcow2 -c appliance/root.raw appliance/root.qcow2
rm appliance/root.raw
