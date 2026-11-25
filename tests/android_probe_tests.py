import importlib.util
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('probe', pathlib.Path(__file__).with_name('android_boot_probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class RecipeTests(unittest.TestCase):
    def test_preserves_guest_options_and_skips_installer(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            (root / 'kernel').touch()
            (root / 'initrd.img').touch()
            (root / 'grub.cfg').write_text('linux /kernel INSTALL=1\ninitrd /initrd.img\nlinux /kernel quiet root=/dev/ram0 SRC= DATA=\ninitrd /initrd.img\n')
            kernel, initrd, args, config = probe.boot_recipe(root)
            self.assertEqual(kernel.name, 'kernel')
            self.assertEqual(initrd.name, 'initrd.img')
            self.assertNotIn('quiet', args)
            self.assertIn('SRC= DATA=', args)
            self.assertNotIn('INSTALL', args)

    def test_bliss_isolinux_live_recipe(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            (root / 'kernel').touch()
            (root / 'initrd.img').touch()
            (root / 'isolinux.cfg').write_text('label live\n kernel /kernel\n append initrd=/initrd.img root=/dev/ram0 quiet SRC= DATA=\nlabel install\n kernel /kernel\n append initrd=/initrd.img INSTALL=1\n')
            self.assertEqual(probe.boot_recipe(root)[2], 'root=/dev/ram0 SRC= DATA=')

    def test_unknown_recipe_fails_instead_of_guessing(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(RuntimeError):
                probe.boot_recipe(pathlib.Path(folder))


if __name__ == '__main__':
    unittest.main()
