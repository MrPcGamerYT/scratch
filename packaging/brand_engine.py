"""Brand the unsigned embedded Windows engine without hiding QEMU attribution.

Uses the Windows resource API (no external PE editor). Also works on a cached
complete engine bundle; no CPU/device code changes. Run before ZIP checksums.
"""
import ctypes
from ctypes import wintypes
import pathlib
import shutil
import struct
import sys


def wide(value):
    return (value + '\0').encode('utf-16le')


def block(key, value=b'', children=b'', kind=1, length=None):
    data = bytearray(struct.pack('<HHH', 0, len(value) // 2 if length is None else length, kind) + wide(key))
    data.extend(bytes((-len(data)) % 4))
    data.extend(value)
    data.extend(bytes((-len(data)) % 4))
    data.extend(children)
    struct.pack_into('<H', data, 0, len(data))
    return bytes(data)


def version_resource():
    strings = {
        'CompanyName': 'Project Nexus',
        'FileDescription': 'Nexus Engine',
        'FileVersion': '10.2.0.0',
        'InternalName': 'NexusEngine',
        'OriginalFilename': 'NexusEngine.exe',
        'ProductName': 'Nexus Engine (QEMU-based)',
        'ProductVersion': '10.2.0 Nexus release-lto-v1',
        'LegalCopyright': 'QEMU: copyright various authors; GNU GPL. Nexus branding: Project Nexus.',
        'LegalTrademarks': 'QEMU is a trademark of Fabrice Bellard.',
    }
    table = block('040904E4', children=b''.join(block(k, wide(v)) for k, v in strings.items()))
    info = block('StringFileInfo', children=table)
    var = block('VarFileInfo', children=block('Translation', struct.pack('<HH', 0x409, 1252), kind=0, length=4))
    fixed = struct.pack('<13I', 0xFEEF04BD, 0x10000, 10 << 16 | 2, 0, 10 << 16 | 2, 0, 0x3F, 0, 0x40004, 1, 0, 0, 0)
    return block('VS_VERSION_INFO', fixed, info + var, kind=0, length=len(fixed))


def brand(bundle):
    if sys.platform != 'win32':
        raise RuntimeError('Windows resource update APIs are required')
    bundle = pathlib.Path(bundle).resolve()
    assets = pathlib.Path(__file__).resolve().parent / 'assets'
    if not assets.is_dir():
        assets = pathlib.Path(__file__).resolve().parents[1] / 'assets'
    old = bundle / 'bin/qemu-system-x86_64.exe'
    exe = bundle / 'bin/NexusEngine.exe'
    if old.exists():
        if exe.exists():
            raise RuntimeError('Both engine names exist; refusing ambiguous bundle')
        old.rename(exe)
    if not exe.is_file():
        raise FileNotFoundError(exe)
    k = ctypes.WinDLL('kernel32', use_last_error=True)
    k.BeginUpdateResourceW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL]
    k.BeginUpdateResourceW.restype = wintypes.HANDLE
    k.UpdateResourceW.argtypes = [wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, wintypes.WORD, ctypes.c_void_p, wintypes.DWORD]
    k.UpdateResourceW.restype = wintypes.BOOL
    k.EndUpdateResourceW.argtypes = [wintypes.HANDLE, wintypes.BOOL]
    k.EndUpdateResourceW.restype = wintypes.BOOL
    # Delete old icon groups, not unrelated manifests/version translations.
    k.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    k.LoadLibraryExW.restype = wintypes.HMODULE
    k.FreeLibrary.argtypes = [wintypes.HMODULE]
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ssize_t)
    k.EnumResourceNamesW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, callback_type, ctypes.c_ssize_t]
    groups = []
    @callback_type
    def collect(module, kind, name, param):
        groups.append(name if name <= 65535 else ctypes.wstring_at(name))
        return True
    module = k.LoadLibraryExW(str(exe), None, 2)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    k.EnumResourceNamesW(module, 14, collect, 0)
    k.FreeLibrary(module)
    handle = k.BeginUpdateResourceW(str(exe), False)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    def update(kind, name, data):
        resource_name = name if isinstance(name, int) else ctypes.cast(ctypes.c_wchar_p(name), ctypes.c_void_p)
        buffer = ctypes.create_string_buffer(data) if data is not None else None
        if not k.UpdateResourceW(handle, kind, resource_name, 0x409, buffer, len(data) if data is not None else 0):
            raise ctypes.WinError(ctypes.get_last_error())
    try:
        for group in groups:
            update(14, group, None)
        update(16, 1, version_resource())
        icon = (assets / 'nexus-engine.ico').read_bytes()
        reserved, kind, count = struct.unpack_from('<HHH', icon)
        if (reserved, kind) != (0, 1):
            raise ValueError('Invalid ICO')
        group = bytearray(struct.pack('<HHH', 0, 1, count))
        for index in range(count):
            width, height, colors, zero, planes, bits, size, offset = struct.unpack_from('<BBBBHHII', icon, 6 + 16 * index)
            update(3, index + 1, icon[offset:offset + size])
            group += struct.pack('<BBBBHHIH', width, height, colors, zero, planes, bits, size, index + 1)
        update(14, 1, bytes(group))
    except BaseException:
        k.EndUpdateResourceW(handle, True)
        raise
    if not k.EndUpdateResourceW(handle, False):
        raise ctypes.WinError(ctypes.get_last_error())
    # Verify the actual PE icon payload, not merely the presence of any icon.
    k.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    k.FindResourceW.restype = wintypes.HANDLE
    k.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    k.LoadResource.restype = wintypes.HANDLE
    k.LockResource.argtypes = [wintypes.HANDLE]
    k.LockResource.restype = ctypes.c_void_p
    k.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    k.SizeofResource.restype = wintypes.DWORD
    module = k.LoadLibraryExW(str(exe), None, 2)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        resource = k.FindResourceW(module, 1, 3)
        pointer = k.LockResource(k.LoadResource(module, resource)) if resource else None
        actual = ctypes.string_at(pointer, k.SizeofResource(module, resource)) if pointer else b''
        first = struct.unpack_from('<BBBBHHII', icon, 6)
        if actual != icon[first[7]:first[7] + first[6]]:
            raise RuntimeError('Embedded PE icon did not match the Nexus asset')
    finally:
        k.FreeLibrary(module)
    for size, extension in [(128, 'png'), (32, 'bmp')]:
        target = bundle / f'share/icons/hicolor/{size}x{size}/apps/qemu.{extension}'
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(assets / f'nexus-engine.{extension}', target)
    sources = bundle / 'sources/nexus-branding'
    sources.mkdir(parents=True, exist_ok=True)
    if pathlib.Path(__file__).resolve() != (sources / 'brand_engine.py').resolve():
        shutil.copyfile(__file__, sources / 'brand_engine.py')
    (sources / 'assets').mkdir(exist_ok=True)
    for asset in assets.glob('nexus-engine.*'):
        if asset.resolve() != (sources / 'assets' / asset.name).resolve():
            shutil.copyfile(asset, sources / 'assets' / asset.name)
    print('Branded NexusEngine.exe: Nexus Engine description and Nexus multi-resolution icon; QEMU attribution preserved.')


if __name__ == '__main__':
    brand(sys.argv[1])
