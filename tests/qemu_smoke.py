"""Boot a tiny real x86 sector using QEMU TCG, independently of Android/WHPX."""
import os
import pathlib
import subprocess
import tempfile

with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory)
    disk = root / 'test.qcow2'
    subprocess.run([os.environ.get('QEMU_IMG', 'qemu-img'), 'create', '-f', 'qcow2', str(disk), '16M'], check=True)
    info = subprocess.run([os.environ.get('QEMU_IMG', 'qemu-img'), 'info', '--output=json', str(disk)], check=True, capture_output=True, text=True)
    import json
    assert json.loads(info.stdout)['format'] == 'qcow2'
    # 16-bit real-mode code prints to QEMU's debug console then halts.
    code = bytearray(b'\xba\xe9\x00')  # mov dx, 0xe9
    for char in b'NEXUS_BOOT_OK':
        code.extend((0xb0, char, 0xee))  # mov al, char; out dx, al
    code.extend(b'\xfa\xf4\xeb\xfd')  # cli; hlt; loop
    code.extend(b'\x00' * (510 - len(code)))
    code.extend(b'\x55\xaa')
    boot = root / 'boot.img'
    boot.write_bytes(code + b'\x00' * (1474560 - len(code)))
    firmware = ['-L', os.environ['QEMU_DATA']] if os.environ.get('QEMU_DATA') else []
    process = subprocess.Popen([os.environ.get('QEMU_SYSTEM', 'qemu-system-x86_64'), *firmware, '-accel', 'tcg', '-m', '64', '-display', 'none', '-monitor', 'none', '-nic', 'none', '-serial', 'none', '-debugcon', 'stdio', '-drive', f'format=raw,if=floppy,file={boot}', '-boot', 'a', '-no-reboot'], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = process.communicate(timeout=12)
    except subprocess.TimeoutExpired:
        process.kill()
        stdout, stderr = process.communicate()
    if b'NEXUS_BOOT_OK' not in stdout:
        raise RuntimeError(f'QEMU did not execute the test boot sector: {stdout!r} {stderr!r}')
    print('Real QEMU TCG boot and QCOW2 smoke test passed (not Android or WHPX).')
