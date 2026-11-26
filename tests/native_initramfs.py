"""Deterministic dependency-free Linux diagnostic PID1 ELF/newc fixture.
Not Android, a shell, an installer or a production init system.
PID1 proves getpid and two real nanosleep returns via /dev/kmsg markers.
"""
import struct


def init_elf(disk_write=None, filesystem=False, clean_unmount=True, display=False, bridge=False):
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
    # Exercise the real kernel clock syscall, not RDTSC or a host timestamp.
    # Stack output remains writable even for the default read/execute ELF.
    emit(b'\x48\x83\xec\x10\x45\x31\xf6')  # reserve timespec; r14d=fine delta seen
    def clock_sample():
        emit(b'\xb8\xe4\x00\x00\x00\xbf\x01\x00\x00\x00\x48\x89\xe6\x0f\x05\x85\xc0')
        branch(b'\x0f\x85','fail')
        emit(b'\x48\x8b\x04\x24\x48\x69\xc0'+struct.pack('<I',1_000_000_000)+b'\x48\x03\x44\x24\x08')
    clock_sample()
    emit(b'\x49\x89\xc4\xbb\x00\x10\x00\x00')  # previous timestamp, 4096 samples
    label('clocksample')
    clock_sample()
    emit(b'\x4c\x39\xe0');branch(b'\x0f\x82','fail')  # no backward clock
    emit(b'\x49\x89\xc5\x4c\x29\xe0\x48\x85\xc0')
    branch(b'\x0f\x84','clocknext')  # equality allowed, but not for every sample
    emit(b'\x48\x3d'+struct.pack('<I',1_000_000));branch(b'\x0f\x83','clocknext')
    emit(b'\x41\xbe\x01\x00\x00\x00')  # observed positive delta below 1 ms
    label('clocknext')
    emit(b'\x4d\x89\xec\xff\xcb');branch(b'\x0f\x85','clocksample')
    emit(b'\x45\x85\xf6');branch(b'\x0f\x84','fail')
    emit(b'\x48\x83\xc4\x10')
    clockmarker=b'NEXUS_MONOTONIC_4096_NO_BACKWARD_SUBMS_DELTA\n'
    log('clockmarker',clockmarker)
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
        emit(b'\xbe\x02\x00\x00\x00\x31\xd2\x0f\x05\x85\xc0')
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
        # Tint each next row differently so host comparison catches stride/vertical flips.
        address(b'\x48\xbb', 'fbrow')
        emit(b'\x48\xff\xc3\xb9\x20\x03\x00\x00')
        label('tintrow')
        emit(b'\xfe\x03\x48\x83\xc3\x04\xff\xc9')
        branch(b'\x0f\x85', 'tintrow')
        emit(b'\x41\xff\xcd')
        branch(b'\x0f\x85', 'drawrow')
        # mmap the actual fbdev surface and overwrite the final pixel through guest paging.
        emit(b'\xb8\x09\x00\x00\x00\x31\xff\xbe\x00\x50\x1d\x00\xba\x03\x00\x00\x00')
        emit(b'\x41\xba\x01\x00\x00\x00\x45\x89\xe0\x45\x31\xc9\x0f\x05\x48\x85\xc0')
        branch(b'\x0f\x88', 'fail')
        emit(b'\x49\x89\xc6\x41\xc7\x86'+struct.pack('<I',800*600*4-4)+b'\xde\xad\xbe\xef')
        emit(b'\x41\x81\xbe'+struct.pack('<I',800*600*4-4)+b'\xde\xad\xbe\xef')
        branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x0b\x00\x00\x00\x4c\x89\xf7\xbe\x00\x50\x1d\x00\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        emit(b'\xb8\x03\x00\x00\x00\x44\x89\xe7\x0f\x05\x85\xc0')
        branch(b'\x0f\x85', 'fail')
        display_marker=b'NEXUS_FB0_PATTERN_WRITTEN\n'
        log('displaymarker',display_marker)
    if bridge:
        emit(b'\xb8\x02\x00\x00\x00')
        address(b'\x48\xbf','bridgepath')
        emit(b'\xbe\x02\x00\x00\x00\x31\xd2\x0f\x05\x85\xc0')
        branch(b'\x0f\x88','fail')
        emit(b'\x41\x89\xc4')  # r12d=private tty fd
        emit(b'\xb8\x10\x00\x00\x00\x44\x89\xe7\xbe\x02\x54\x00\x00')
        address(b'\x48\xba','bridgetermios')
        emit(b'\x0f\x05\x85\xc0');branch(b'\x0f\x85','fail')
        def bridge_write(name,body):
            emit(b'\xb8\x01\x00\x00\x00\x44\x89\xe7')
            address(b'\x48\xbe',name)
            emit(b'\xba'+struct.pack('<I',len(body))+b'\x0f\x05\x3d'+struct.pack('<I',len(body)))
            branch(b'\x0f\x85','fail')
        greeting=b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK\n'
        answer=b'NEXUS 1 APP org.nexus.fixture\nNEXUS 1 RESULT QUERY_APP OK\n'
        query=b'NEXUS 1 QUERY_APP\n'
        bridge_write('bridgehello',greeting)
        address(b'\x49\xbe','bridgequery')  # r14 expected pointer
        emit(b'\x41\xbf'+struct.pack('<I',len(query)))
        label('bridgeread')
        emit(b'\x31\xc0\x44\x89\xe7');address(b'\x48\xbe','bridgebyte')
        emit(b'\xba\x01\x00\x00\x00\x0f\x05\x48\x83\xf8\x01');branch(b'\x0f\x85','fail')
        address(b'\x48\xbb','bridgebyte');emit(b'\x8a\x03\x41\x3a\x06');branch(b'\x0f\x85','fail')
        emit(b'\x49\xff\xc6\x41\xff\xcf');branch(b'\x0f\x85','bridgeread')
        bridge_marker=b'NEXUS_PRIVATE_COM2_EXACT_QUERY_MATCH\n'
        log('bridgemarker',bridge_marker)
        bridge_write('bridgeanswer',answer)
    log('ready', ready)
    label('pause')
    emit(b'\xb8\x22\x00\x00\x00\x0f\x05')
    branch(b'\xe9', 'pause')
    label('fail')
    emit(b'\x0f\x0b')  # visible PID1 failure, never a success marker
    for name, data in [('device', b'/dev/kmsg\0'), ('timespec', struct.pack('<qq', 0, 100_000_000)),
                       ('started', started), ('tick', tick), ('ready', ready), ('clockmarker',clockmarker)]:
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
    if bridge:
        cc=bytearray(20);cc[7]=1  # c_line then NCCS: VMIN index 6.
        for name,body in [('bridgepath',b'/dev/ttyS1\0'),('bridgetermios',struct.pack('<IIII',0,0,0x8bd,0)+cc),
                          ('bridgehello',greeting),('bridgequery',query),('bridgeanswer',answer),
                          ('bridgebyte',bytes(1)),('bridgemarker',bridge_marker)]:
            label(name);emit(body)
    for offset, name in absolute:
        struct.pack_into('<Q', code, offset, entry + labels[name])
    for offset, name in relative:
        struct.pack_into('<i', code, offset, labels[name] - offset - 4)
    size = 120 + len(code)
    header = struct.pack('<16sHHIQQQIHHHHHH', b'\x7fELF\x02\x01\x01'+bytes(9), 2, 62, 1,
                         entry, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    segment = struct.pack('<IIQQQQQQ', 1, 7 if disk_write is not None or display or bridge else 5, 0, base, base, size, size, 4096)
    return header + segment + code


def newc(name, mode, body=b'', rmajor=0, rminor=0, inode=1):
    encoded = name.encode() + b'\0'
    fields = [inode, mode, 0, 0, 1, 0, len(body), 0, 0, rmajor, rminor, len(encoded), 0]
    data = b'070701' + b''.join(f'{v:08x}'.encode() for v in fields) + encoded
    data += bytes(-len(data) % 4)
    data += body
    return data + bytes(-len(data) % 4)


def initramfs(disk_write=None, filesystem=False, clean_unmount=True, display=False, bridge=False):
    return (newc('dev', 0o40755, inode=1) + newc('dev/console', 0o20600, rmajor=5, rminor=1, inode=2)
            + newc('dev/kmsg', 0o20600, rmajor=1, rminor=11, inode=3)
            + (newc('dev/ttyS1',0o20600,rmajor=4,rminor=65,inode=7) if bridge else b'')
            + (newc('mnt', 0o40755, inode=6) if filesystem else b'')
            + newc('init', 0o100755, init_elf(disk_write, filesystem, clean_unmount, display, bridge), inode=4) + newc('TRAILER!!!', 0, inode=5))


if __name__ == '__main__':
    import argparse
    from pathlib import Path
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('output')
    args = p.parse_args()
    with Path(args.output).open('xb') as stream:
        stream.write(initramfs())
