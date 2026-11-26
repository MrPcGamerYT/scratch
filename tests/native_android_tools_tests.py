"""Host-only Android payload/audit guards; not Android boot tests."""
import gzip
from pathlib import Path
import struct
import tempfile
import unittest
from native_android_assets import manifest
from native_android_contract import archive,inspect
from native_initramfs import newc


class AndroidToolTests(unittest.TestCase):
    def test_manifest_fingerprints_originals_without_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);kernel=bytearray(4096)
            kernel[0x1fe:0x200]=b'\x55\xaa';kernel[0x202:0x206]=b'HdrS'
            struct.pack_into('<H',kernel,0x206,0x20f)
            (root/'kernel').write_bytes(kernel);(root/'initrd.img').write_bytes(b'copied-ramdisk')
            before={p.name:p.read_bytes() for p in root.iterdir()}
            data=manifest(root)
            self.assertEqual(data['linux_boot_protocol'],'0x20f')
            self.assertFalse(data['android_init_executed']);self.assertFalse(data['android_home_verified'])
            self.assertEqual(before,{p.name:p.read_bytes() for p in root.iterdir()})
            (root/'kernel').write_bytes(b'invalid')
            with self.assertRaises(ValueError):
                manifest(root)

    def test_invalid_or_missing_boot_files_are_not_guessed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            with self.assertRaises(ValueError):
                manifest(root)
            (root/'kernel').write_bytes(b'bad')
            (root/'initrd.img').write_bytes(b'')
            with self.assertRaises(ValueError):
                manifest(root)

    def test_archive_duplicates_are_reported_without_extraction(self):
        raw=(newc('lib',0o40755)+newc('./lib',0o40700)+newc('init',0o100755,b'#!/bin/sh\n')+newc('TRAILER!!!',0))
        for body in [raw,gzip.compress(raw)]:
            files,duplicates=archive(body)
            self.assertEqual(duplicates,['lib'])
            self.assertEqual(files['lib'][0],0o40700)
            self.assertEqual(files['init'][1],b'#!/bin/sh\n')
        for bad in [b'not cpio',raw[:109],newc('init',0o100755,b'abc')[:-2]]:
            with self.assertRaises(ValueError):
                archive(bad)

    def test_driver_contract_distinguishes_builtin_module_and_unknown(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            self.assertEqual(inspect(root)['driver_configuration']['VIRTIO'],'unknown')
            (root/'kernel.config').write_text('CONFIG_VIRTIO=y\nCONFIG_VIRTIO_CONSOLE=m\n# CONFIG_FB_SIMPLE is not set\n')
            report=inspect(root)
            self.assertEqual(report['driver_configuration']['VIRTIO'],'y')
            self.assertEqual(report['driver_configuration']['VIRTIO_CONSOLE'],'m')
            self.assertEqual(report['driver_configuration']['FB_SIMPLE'],'not set')
            self.assertFalse(report['android_home_verified'])


if __name__=='__main__':
    unittest.main()
