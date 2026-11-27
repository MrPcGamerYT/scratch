#!/usr/bin/env python3
"""Inspect/extract full Android OTA payloads. Never creates a bootable PC disk.

Standard-library protobuf wire reader; supports full-image REPLACE, REPLACE_BZ,
REPLACE_XZ, ZERO and DISCARD operations. Incremental operations are refused.
"""
import argparse
import bz2
import hashlib
import json
import lzma
import os
from pathlib import Path
import re
import shutil
import struct
import tempfile
import zipfile

MAX_MANIFEST = 64 * 1024 * 1024
MAX_OPERATION = 256 * 1024 * 1024
SUPPORTED = {0, 1, 6, 7, 8}


def varint(data, offset):
    value = 0
    for shift in range(0, 70, 7):
        if offset >= len(data):
            raise ValueError('Truncated protobuf varint')
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if not byte & 128:
            return value, offset
    raise ValueError('Oversized protobuf varint')


def fields(data):
    result = {}
    offset = 0
    while offset < len(data):
        tag, offset = varint(data, offset)
        number, wire = tag >> 3, tag & 7
        if not number:
            raise ValueError('Invalid protobuf field')
        if wire == 0:
            value, offset = varint(data, offset)
        elif wire in (1, 5):
            size = 8 if wire == 1 else 4
            value = data[offset:offset + size]
            offset += size
            if len(value) != size:
                raise ValueError('Truncated fixed field')
        elif wire == 2:
            size, offset = varint(data, offset)
            value = data[offset:offset + size]
            offset += size
            if len(value) != size:
                raise ValueError('Truncated protobuf field')
        else:
            raise ValueError('Unsupported protobuf wire type')
        result.setdefault(number, []).append(value)
    return result


def one(message, key, default=None):
    values = message.get(key, [])
    if len(values) > 1:
        raise ValueError('Duplicate singular manifest field')
    return values[0] if values else default


def inspect_payload(stream):
    header = stream.read(20)
    if len(header) != 20 or header[:4] != b'CrAU':
        raise ValueError('Not an Android OTA payload (CrAU header missing)')
    version, length = struct.unpack('>QQ', header[4:])
    if version not in (1, 2) or not 0 < length <= MAX_MANIFEST:
        raise ValueError('Unsupported payload version or manifest size')
    signature_size = 0
    if version == 2:
        raw = stream.read(4)
        if len(raw) != 4:
            raise ValueError('Truncated payload header')
        signature_size = struct.unpack('>I', raw)[0]
    manifest_data = stream.read(length)
    if len(manifest_data) != length:
        raise ValueError('Truncated OTA manifest')
    manifest = fields(manifest_data)
    block = one(manifest, 3, 4096)
    if not isinstance(block, int) or block < 512 or block > 65536 or block & (block - 1):
        raise ValueError('Invalid OTA block size')
    data_start = 20 + (4 if version == 2 else 0) + length + signature_size
    stream.seek(0, os.SEEK_END)
    size = stream.tell()
    if data_start > size:
        raise ValueError('Truncated metadata signature')
    partitions = []
    seen = set()
    for raw in manifest.get(13, []):
        partition = fields(raw)
        name = one(partition, 1, b'').decode('ascii')
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,100}', name) or name in seen:
            raise ValueError('Unsafe or duplicate partition name')
        seen.add(name)
        info = fields(one(partition, 7, b''))
        image_size = one(info, 1, 0)
        digest = one(info, 2, b'')
        if not isinstance(image_size, int) or not 0 < image_size <= 128 * 1024**3 or (digest and len(digest) != 32):
            raise ValueError('Invalid new partition size/hash')
        operations = []
        for operation_raw in partition.get(8, []):
            operation = fields(operation_raw)
            kind = one(operation, 1)
            offset, length = one(operation, 2, 0), one(operation, 3, 0)
            if not isinstance(kind, int) or not isinstance(offset, int) or not isinstance(length, int):
                raise ValueError('Invalid operation metadata')
            if offset + length > size - data_start or length > MAX_OPERATION:
                raise ValueError('Operation data exceeds payload bounds/safety limit')
            extents = []
            for extent_raw in operation.get(6, []):
                extent = fields(extent_raw)
                start, count = one(extent, 1), one(extent, 2)
                if not isinstance(start, int) or not isinstance(count, int) or count <= 0 or (start + count) * block > ((image_size + block - 1) // block) * block:
                    raise ValueError('Destination extent outside partition')
                extents.append((start * block, count * block))
            if not extents:
                raise ValueError('Operation has no destination extents')
            operations.append({'type': kind, 'offset': offset, 'length': length, 'extents': extents, 'hash': one(operation, 8, b'')})
        if not operations:
            raise ValueError('Partition has no extraction operations')
        partitions.append({'name': name, 'size': image_size, 'hash': digest, 'operations': operations})
    if not partitions:
        raise ValueError('No modern partition manifest found; legacy payload layout is not supported')
    return data_start, partitions


def decoded_chunks(stream, operation, data_start):
    kind = operation['type']
    remaining_output = sum(length for _, length in operation['extents'])
    if kind in (6, 7):
        while remaining_output:
            chunk = b'\0' * min(1024 * 1024, remaining_output)
            remaining_output -= len(chunk)
            yield chunk
        return
    stream.seek(data_start + operation['offset'])
    data = stream.read(operation['length'])
    if len(data) != operation['length']:
        raise ValueError('Truncated operation data')
    if operation['hash'] and hashlib.sha256(data).digest() != operation['hash']:
        raise ValueError('Operation SHA-256 mismatch')
    if kind == 0:
        if len(data) != remaining_output:
            raise ValueError('REPLACE length does not match destination extents')
        yield data
        return
    decoder = bz2.BZ2Decompressor() if kind == 1 else lzma.LZMADecompressor(memlimit=256 * 1024**2)
    first = True
    while not decoder.eof:
        chunk = decoder.decompress(data if first else b'', max_length=min(1024 * 1024, remaining_output + 1))
        first = False
        remaining_output -= len(chunk)
        if remaining_output < 0:
            raise ValueError('Compressed operation exceeds destination size')
        if chunk:
            yield chunk
        if decoder.needs_input and not decoder.eof:
            raise ValueError('Truncated compressed operation')
    if remaining_output or decoder.unused_data:
        raise ValueError('Compressed operation size/trailing-data mismatch')


def extract(stream, destination):
    data_start, partitions = inspect_payload(stream)
    unsupported = sorted({op['type'] for part in partitions for op in part['operations']} - SUPPORTED)
    if unsupported:
        raise ValueError(f'Incremental/unsupported OTA operations {unsupported}. Obtain a full OTA or a bootable Bliss installer. No output created.')
    destination = Path(destination)
    if destination.exists():
        raise FileExistsError('Output folder already exists; choose a new extraction folder')
    destination.parent.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(destination.parent).free < sum(p['size'] for p in partitions) + 64 * 1024**2:
        raise ValueError('Insufficient space for extracted partitions')
    temporary = Path(tempfile.mkdtemp(prefix='.ota-extract-', dir=destination.parent))
    try:
        for part in partitions:
            print(f"Extracting {part['name']} ({part['size']} bytes)", flush=True)
            with open(temporary / (part['name'] + '.img'), 'xb+') as output:
                for operation in part['operations']:
                    extents = iter(operation['extents'])
                    position, remaining = next(extents)
                    output.seek(position)
                    for chunk in decoded_chunks(stream, operation, data_start):
                        offset = 0
                        while offset < len(chunk):
                            count = min(remaining, len(chunk) - offset)
                            output.write(chunk[offset:offset + count])
                            offset += count
                            remaining -= count
                            if not remaining:
                                try:
                                    position, remaining = next(extents)
                                    output.seek(position)
                                except StopIteration:
                                    if offset != len(chunk):
                                        raise ValueError('Operation overflow')
                    if remaining:
                        raise ValueError('Operation did not fill destination')
                output.truncate(part['size'])
                output.seek(0)
                digest = hashlib.sha256()
                for chunk in iter(lambda: output.read(1024 * 1024), b''):
                    digest.update(chunk)
                if part['hash'] and digest.digest() != part['hash']:
                    raise ValueError(f"Partition SHA-256 mismatch: {part['name']}")
        (temporary / 'README.txt').write_text('Extracted OTA partition images, NOT a bootable PC disk.\nDo not rename system.img to android-base.qcow2.\nUse a matching bootloader/partition layout or obtain the Bliss PC installer.\nOriginal package signatures were NOT authenticated. Verify the source with its publisher.\n', encoding='utf-8')
        if destination.exists():
            raise FileExistsError('Destination was created during extraction')
        temporary.rename(destination)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    return partitions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--sha256')
    args = parser.parse_args()
    if args.sha256:
        if not re.fullmatch(r'[0-9a-fA-F]{64}', args.sha256):
            raise ValueError('Expected SHA-256 must have 64 hexadecimal characters')
        digest = hashlib.sha256()
        with open(args.source, 'rb') as source_stream:
            for chunk in iter(lambda: source_stream.read(1024 * 1024), b''):
                digest.update(chunk)
        if digest.hexdigest().lower() != args.sha256.lower():
            raise ValueError('Source SHA-256 mismatch')
    with tempfile.TemporaryDirectory(prefix='nexus-payload-') as directory:
        source = args.source
        if zipfile.is_zipfile(source):
            with zipfile.ZipFile(source) as archive:
                candidates = [entry for entry in archive.infolist() if not entry.is_dir() and entry.filename.replace('\\', '/').split('/')[-1] == 'payload.bin']
                if len(candidates) != 1:
                    raise ValueError('ZIP must contain exactly one payload.bin')
                entry = candidates[0]
                if entry.file_size > 128 * 1024**3 or entry.file_size > shutil.disk_usage(directory).free - 64 * 1024**2:
                    raise ValueError('Payload too large / insufficient temporary disk space')
                source = Path(directory) / 'payload.bin'
                with archive.open(entry) as inp, open(source, 'xb') as out:
                    shutil.copyfileobj(inp, out, 1024 * 1024)
        with open(source, 'rb') as stream:
            if args.list:
                _, parts = inspect_payload(stream)
            else:
                if not args.output:
                    parser.error('--output is required for extraction')
                parts = extract(stream, args.output)
            print(json.dumps([{'partition': p['name'], 'bytes': p['size'], 'operations': len(p['operations']), 'extractable': all(op['type'] in SUPPORTED for op in p['operations'])} for p in parts], indent=2))
            print('OTA partitions are not a bootable emulator disk. Payload signatures were not authenticated.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, lzma.LZMAError, zipfile.BadZipFile) as error:
        raise SystemExit(str(error))
