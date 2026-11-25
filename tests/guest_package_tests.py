import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('packager', Path(__file__).parents[1] / 'guest-tools/package_candidate.py')
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class PackageTests(unittest.TestCase):
    def fixture(self, root, status='framework_ready_visual_unverified'):
        candidate = root / 'candidate'
        (candidate / 'evidence').mkdir(parents=True)
        disk = candidate / 'android-candidate.qcow2'
        disk.write_bytes(b'fixture, not a real disk')
        (candidate / 'android-candidate.json').write_text(json.dumps({'image_sha256': packager.sha256(disk), 'ready_for_player': False}))
        (candidate / 'evidence/result.json').write_text(json.dumps({'status': status, 'framework_boot_completed': True}))
        inputs = []
        for name in ['code', 'vars', 'notices']:
            path = root / name
            path.write_text(name)
            inputs.append(path)
        return candidate, inputs

    def test_packaged_disk_hash_and_clean_template(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            candidate, inputs = self.fixture(root)
            output = root / 'download'
            packager.package(candidate, output, *inputs)
            self.assertEqual((output / 'uefi-vars-template.fd').read_text(), 'vars')
            self.assertEqual((output / 'android-candidate.qcow2').read_bytes(), (candidate / 'android-candidate.qcow2').read_bytes())
            self.assertIn('android-candidate.qcow2', (output / 'SHA256SUMS.txt').read_text())
            self.assertFalse(json.loads((output / 'compatibility.json').read_text())['ready_for_player'])
            with self.assertRaises(FileExistsError): packager.package(candidate, output, *inputs)

    def test_refuses_failed_boot_and_mutated_base(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            candidate, inputs = self.fixture(root, 'failed')
            with self.assertRaises(ValueError): packager.package(candidate, root / 'download', *inputs)
            (candidate / 'evidence/result.json').write_text(json.dumps({'status': 'framework_ready_visual_unverified', 'framework_boot_completed': True}))
            (candidate / 'android-candidate.qcow2').write_bytes(b'mutated')
            with self.assertRaises(ValueError): packager.package(candidate, root / 'download', *inputs)
            self.assertFalse((root / 'download').exists())


if __name__ == '__main__': unittest.main()
