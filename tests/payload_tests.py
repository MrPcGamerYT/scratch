import bz2
import hashlib
import importlib.util
import io
import lzma
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('payload_tool', Path(__file__).parents[1] / 'image-studio-tools/payload_tool.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def varint(number):
    output = bytearray()
    while number > 127:
        output.append((number & 127) | 128)
        number >>= 7
    output.append(number)
    return bytes(output)


def integer(key, value):
    return varint(key << 3) + varint(value)


def blob(key, value):
    return varint((key << 3) | 2) + varint(len(value)) + value


def payload(kind=0, name=b'system', bad_hash=False):
    content = b'NEXUS' + bytes(507)
    data = {0: lambda: content, 1: lambda: bz2.compress(content), 8: lambda: lzma.compress(content)}.get(kind, lambda: b'')()
    expected = bytes(512) if kind in (6, 7) else content
    digest = hashlib.sha256(expected).digest() if not bad_hash else bytes(32)
    extent = integer(1, 0) + integer(2, 1)
    operation = integer(1, kind) + integer(2, 0) + integer(3, len(data)) + blob(6, extent) + blob(8, hashlib.sha256(data).digest())
    partition = blob(1, name) + blob(7, integer(1,512) + blob(2,digest)) + blob(8,operation)
    manifest = integer(3,512) + blob(13,partition)
    return b'CrAU' + struct.pack('>QQI',2,len(manifest),0) + manifest + data, expected


class PayloadTests(unittest.TestCase):
    def test_full_operations(self):
        for kind in (0,1,6,7,8):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as root:
                source, expected = payload(kind)
                output = Path(root)/'output'
                module.extract(io.BytesIO(source), output)
                self.assertEqual((output/'system.img').read_bytes(),expected)
                with self.assertRaises(FileExistsError):
                    module.extract(io.BytesIO(source),output)

    def test_incremental_refused_before_output(self):
        with tempfile.TemporaryDirectory() as root:
            output=Path(root)/'output'
            with self.assertRaisesRegex(ValueError,'Incremental'):
                module.extract(io.BytesIO(payload(4)[0]),output)
            self.assertFalse(output.exists())

    def test_hash_failure_cleans_partial_output(self):
        with tempfile.TemporaryDirectory() as root:
            output=Path(root)/'output'
            with self.assertRaisesRegex(ValueError,'SHA-256'):
                module.extract(io.BytesIO(payload(bad_hash=True)[0]),output)
            self.assertEqual(list(Path(root).iterdir()),[])

    def test_unsafe_names_and_truncation(self):
        for source in (payload(name=b'../escape')[0],payload()[0][:-1],b'not a payload'):
            with self.assertRaises(ValueError):
                module.inspect_payload(io.BytesIO(source))


if __name__=='__main__':
    unittest.main()
