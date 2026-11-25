import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('builder', Path(__file__).parents[1] / 'guest-tools/prepare_guest.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class BuilderTests(unittest.TestCase):
    def test_requires_real_boot_and_system_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            tree = Path(directory)
            with self.assertRaises(ValueError): builder.payloads(tree)
            for name in ['kernel', 'initrd.img', 'system.sfs', 'ramdisk.img', 'notes.txt']:
                (tree / name).touch()
            self.assertEqual(builder.payloads(tree), ['initrd.img', 'kernel', 'ramdisk.img', 'system.sfs'])

    def test_profiles_never_skip_setup_or_attach_installer(self):
        for profile in ['no-avx', 'drm-gbm', 'drm-gbm-no-avx']:
            config = builder.grub_config(profile)
            self.assertIn('SRC=/android', config)
            self.assertIn('NEXUS_SYS', config)
            self.assertNotIn('INSTALL=', config)
            self.assertNotIn('SETUPWIZARD=0', config)
        with self.assertRaises(KeyError): builder.grub_config('guess')


if __name__ == '__main__': unittest.main()
