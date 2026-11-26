"""Deterministic dependency-free Linux diagnostic PID1 ELF/newc fixture.
Not Android, a shell, an installer or a production init system.
PID1 proves getpid and two real nanosleep returns via /dev/kmsg markers.
"""
import struct


def init_elf(disk_write=None, filesystem=False, clean_unmount=True, display=False):
    if filesystem and disk_write is None:
        raise ValueError("Filesystem fixture requires an explicit write/read mode")
    base = 0x400000
    entry = base + 120
    code = bytearray()
    labels = {}
    absolute = []
    relative = []

    def emit(data): code.extend(data)
    def label(name): labels[name] = len(code)
    def address(prefix, name):
        emit(prefix)
        absolute.append((len(code), name))
        emit(bytes(8))
    def branch(op, name):
        emit(op)
        relative.append((len(code), name))
        emit(bytes(4))
    def log(name, data):
        emit(b'\xb8\x01\x00\x00\x00\x89\xef')  # write(fd saved in EBP)
        address(b'\x48\xbe', name)
        emit(b'\xba' + struct.pack('<I', len(data)) + b'\x0f\x05')
        emit(b'\x3d' + struct.pack('<I', len(data)))
        branch(b'\x0f\x85', 'fail')

    emit(b'\xb8\x27\x00\x00\x00\x0f\x05\x83\xf8\x01')  # getpid() == 1
    branch(b'\x0f\x85', 'fail')
    emit(b'\xb8\x02\x00\x00\x00')  # open /dev/kmsg O_WRONLY
    address(b'\x48\xbf', 'device')
    emit(b'\xbe\x01\x00\x00\x00\x31\xd2\x0f\x05\x85\xc0')
    branch(b'\x0f\x88', 'fail')
    emit(b'\x89\xc5')  # EBP=fd
    started = b'NEXUS_USERSPACE_PID1\n'
    tick = b'NEXUS_USERSPACE_TICK\n'
    ready = b'NEXUS_USERSPACE_READY\n'
    log('started', started)
    emit(b'\xbb\x02\x00\x00\x00')  # two successful sleeps
    label('sleep')
    emit(b'\xb8\x23\x00\x00\x00')
    address(b'\x48\xbf', 'timespec')
    emit(b'\x31\xf6\x0f\x05\x83\xf8\xfc')  # EINTR: retry, never count as success
    branch(b'\x0f\x84', 'sleep')
    emit(b'\x85\xc0')
    branch(b'\x0f\x85', 'fail')
    log('tick', tick)
    emit(b'\xff\xcb')
    branch(b'\x0f\x85', 'sleep')
    if disk_write is not None:
        # Mount devtmpfs instead of guessing virtio-blk's dynamically assigned major.
        emit(b'\xb8\xa5\x00\x00\x00\x31\xff')
        address(b'\x48\xbe', 'devdir')
        address(b'\x48\xba', 'devfs')
        emit(b'\x45\x31\xd2\x45\x31\xc0\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        if filesystem:
            emit(b'\xb8\xa5\x00\x00\x00')
            address(b'\x48\xbf', 'blockdev')
            address(b'\x48\xbe', 'mountpoint')
            address(b'\x48\xba', 'fstype')
            emit(b'\x41\xba'+struct.pack('<I', 0 if disk_write else 1))
            emit(b'\x45\x31\xc0\x0f\x05\x85\xc0')
            branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x02\x00\x00\x00')
        address(b'\x48\xbf', 'filename' if filesystem else 'blockdev')
        emit(b'\xbe'+struct.pack('<I', (0x242 if disk_write else 0) if filesystem else 2))
        emit(b'\xba\x80\x01\x00\x00\x0f\x05\x85\xc0')
        branch(b'\x0f\x88', 'fail')
        emit(b'\x41\x89\xc4')  # r12d = block fd, ebp still holds kmsg fd
        if disk_write:
            emit(b'\xb8\x12\x00\x00\x00\x44\x89\xe7')  # pwrite64
            address(b'\x48\xbe', 'pattern')
            emit(b'\xba\x00\x02\x00\x00\x41\xba'+struct.pack('<I', 0 if filesystem else 4096)+b'\x0f\x05\x3d\x00\x02\x00\x00')
            branch(b'\x0f\x85', 'fail')
            emit(b'\xb8\x4a\x00\x00\x00\x44\x89\xe7\x0f\x05\x85\xc0')  # fsync
            branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x11\x00\x00\x00\x44\x89\xe7')  # pread64
        address(b'\x48\xbe', 'readbuffer')
        emit(b'\xba\x00\x02\x00\x00\x41\xba'+struct.pack('<I', 0 if filesystem else 4096)+b'\x0f\x05\x3d\x00\x02\x00\x00')
        branch(b'\x0f\x85', 'fail')
        address(b'\x48\xbe', 'pattern')
        address(b'\x48\xbf', 'readbuffer')
        emit(b'\xb9\x00\x02\x00\x00\xfc\xf3\xa6')
        branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x03\x00\x00\x00\x44\x89\xe7\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        if filesystem:
            if disk_write:
                # fsync the parent directory as well as the file, then cleanly unmount.
                emit(b'\xb8\x02\x00\x00\x00')
                address(b'\x48\xbf', 'mountpoint')
                emit(b'\xbe\x00\x00\x01\x00\x31\xd2\x0f\x05\x85\xc0')
                branch(b'\x0f\x88', 'fail')
                emit(b'\x41\x89\xc4\x89\xc7\xb8\x4a\x00\x00\x00\x0f\x05\x85\xc0')
                branch(b'\x0f\x85', 'fail')
                emit(b'\x44\x89\xe7\xb8\x03\x00\x00\x00\x0f\x05\x85\xc0')
                branch(b'\x0f\x85', 'fail')
            if clean_unmount:
                emit(b'\xb8\xa6\x00\x00\x00')
                address(b'\x48\xbf', 'mountpoint')
                emit(b'\x31\xf6\x0f\x05\x85\xc0')
                branch(b'\x0f\x85', 'fail')
        disk_marker=(b'NEXUS_EXT4_FILE_FSYNC_UNMOUNT_MATCH\n' if disk_write else b'NEXUS_EXT4_REBOOT_FILE_MATCH\n') if filesystem else (b'NEXUS_VDA_WRITE_FLUSH_READ_MATCH\n' if disk_write else b'NEXUS_VDA_REBOOT_READ_MATCH\n')
        if filesystem and disk_write and not clean_unmount:
            disk_marker=b'NEXUS_EXT4_FSYNC_UNCLEAN_STOP_READY\n'
        log('diskmarker', disk_marker)
    if display:
        if disk_write is None:
            emit(b'\xb8\xa5\x00\x00\x00\x31\xff')
            address(b'\x48\xbe', 'devdir')
            address(b'\x48\xba', 'devfs')
            emit(b'\x45\x31\xd2\x45\x31\xc0\x0f\x05\x85\xc0')
            branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x02\x00\x00\x00')
        address(b'\x48\xbf', 'fbdev')
        emit(b'\xbe\x01\x00\x00\x00\x31\xd2\x0f\x05\x85\xc0')
        branch(b'\x0f\x88', 'fail')
        emit(b'\x41\x89\xc4\x89\xc7\xb8\x10\x00\x00\x00\xbe\x00\x46\x00\x00')
        address(b'\x48\xba', 'fbinfo')
        emit(b'\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        address(b'\x48\xbb', 'fbinfo')
        for instruction in (b'\x81\x3b\x20\x03\x00\x00',b'\x81\x7b\x04\x58\x02\x00\x00',b'\x81\x7b\x18\x20\x00\x00\x00'):
            emit(instruction)
            branch(b'\x0f\x85', 'fail')
        emit(b'\x41\xbd\x58\x02\x00\x00')
        label('drawrow')
        emit(b'\xb8\x01\x00\x00\x00\x44\x89\xe7')
        address(b'\x48\xbe', 'fbrow')
        emit(b'\xba\x80\x0c\x00\x00\x0f\x05\x3d\x80\x0c\x00\x00')
        branch(b'\x0f\x85', 'fail')
        emit(b'\x41\xff\xcd')
        branch(b'\x0f\x85', 'drawrow')
        emit(b'\xb8\x03\x00\x00\x00\x44\x89\xe7\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        display_marker=b'NEXUS_FB0_PATTERN_WRITTEN\n'
        log('displaymarker',display_marker)
    log('ready', ready)
    label('pause')
    emit(b'\xb8\x22\x00\x00\x00\x0f\x05')
    branch(b'\xe9', 'pause')
    label('fail')
    emit(b'\x0f\x0b')  # visible PID1 failure, never a success marker
    for name, data in [('device', b'/dev/kmsg\0'), ('timespec', struct.pack('<qq', 0, 100_000_000)),
                       ('started', started), ('tick', tick), ('ready', ready)]:
        label(name)
        emit(data)
    if disk_write is not None:
        for name,data in [('devdir',b'/dev\0'),('devfs',b'devtmpfs\0'),('blockdev',b'/dev/vda\0'),
                          ('pattern',bytes((n*37+9)%256 for n in range(512))),('readbuffer',bytes(512)),('diskmarker',disk_marker)]:
            label(name)
            emit(data)
    if filesystem:
        for name,data in [('mountpoint',b'/mnt\0'),('fstype',b'ext4\0'),('filename',b'/mnt/probe.bin\0')]:
            label(name)
            emit(data)
    if display:
        if disk_write is None:
            for name,data in [('devdir',b'/dev\0'),('devfs',b'devtmpfs\0')]:
                label(name)
                emit(data)
        for name,data in [('fbdev',b'/dev/fb0\0'),('fbinfo',bytes(160)),
                          ('fbrow',bytes(component for x in range(800) for component in ((x*3)%256,(x*5)%256,(x*7)%256,255))),
                          ('displaymarker',display_marker)]:
            label(name)
            emit(data)
    for offset, name in absolute:
        struct.pack_into('<Q', code, offset, entry + labels[name])
    for offset, name in relative:
        struct.pack_into('<i', code, offset, labels[name] - offset - 4)
    size = 120 + len(code)
    header = struct.pack('<16sHHIQQQIHHHHHH', b'\x7fELF\x02\x01\x01'+bytes(9), 2, 62, 1,
                         entry, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    segment = struct.pack('<IIQQQQQQ', 1, 7 if disk_write is not None or display else 5, 0, base, base, size, size, 4096)
    return header + segment + code


def newc(name, mode, body=b'', rmajor=0, rminor=0, inode=1):
    encoded = name.encode() + b'\0'
    fields = [inode, mode, 0, 0, 1, 0, len(body), 0, 0, rmajor, rminor, len(encoded), 0]
    data = b'070701' + b''.join(f'{v:08x}'.encode() for v in fields) + encoded
    data += bytes(-len(data) % 4)
    data += body
    return data + bytes(-len(data) % 4)


def initramfs(disk_write=None, filesystem=False, clean_unmount=True, display=False):
    return (newc('dev', 0o40755, inode=1) + newc('dev/console', 0o20600, rmajor=5, rminor=1, inode=2)
            + newc('dev/kmsg', 0o20600, rmajor=1, rminor=11, inode=3)
            + (newc('mnt', 0o40755, inode=6) if filesystem else b'')
            + newc('init', 0o100755, init_elf(disk_write, filesystem, clean_unmount, display), inode=4) + newc('TRAILER!!!', 0, inode=5))


if __name__ == '__main__':
    import argparse
    from pathlib import Path
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('output')
    args = p.parse_args()
    with Path(args.output).open('xb') as stream:
        stream.write(initramfs())
