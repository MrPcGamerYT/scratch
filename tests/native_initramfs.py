"""Deterministic dependency-free Linux diagnostic PID1 ELF/newc fixture.
Not Android, a shell, an installer or a production init system.
PID1 proves getpid and two real nanosleep returns via /dev/kmsg markers.
"""
import struct


def init_elf():
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
    for offset, name in absolute:
        struct.pack_into('<Q', code, offset, entry + labels[name])
    for offset, name in relative:
        struct.pack_into('<i', code, offset, labels[name] - offset - 4)
    size = 120 + len(code)
    header = struct.pack('<16sHHIQQQIHHHHHH', b'\x7fELF\x02\x01\x01'+bytes(9), 2, 62, 1,
                         entry, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    segment = struct.pack('<IIQQQQQQ', 1, 5, 0, base, base, size, size, 4096)
    return header + segment + code


def newc(name, mode, body=b'', rmajor=0, rminor=0, inode=1):
    encoded = name.encode() + b'\0'
    fields = [inode, mode, 0, 0, 1, 0, len(body), 0, 0, rmajor, rminor, len(encoded), 0]
    data = b'070701' + b''.join(f'{v:08x}'.encode() for v in fields) + encoded
    data += bytes(-len(data) % 4)
    data += body
    return data + bytes(-len(data) % 4)


def initramfs():
    return (newc('dev', 0o40755, inode=1) + newc('dev/console', 0o20600, rmajor=5, rminor=1, inode=2)
            + newc('dev/kmsg', 0o20600, rmajor=1, rminor=11, inode=3)
            + newc('init', 0o100755, init_elf(), inode=4) + newc('TRAILER!!!', 0, inode=5))


if __name__ == '__main__':
    import argparse
    from pathlib import Path
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('output')
    args = p.parse_args()
    with Path(args.output).open('xb') as stream:
        stream.write(initramfs())
