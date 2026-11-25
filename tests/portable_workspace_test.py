"""Host-side safety checks. These do NOT certify a portable Windows build."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('portable_workspace','image-studio-tools/portable_workspace.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

class SafetyTests(unittest.TestCase):
    def test_paths(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            self.assertEqual(m.safe_path(root,'/system/app'),root/'system/app')
            for name in ('../outside','system/../../outside'):
                with self.assertRaises(ValueError): m.safe_path(root,name)
            (root/'link').symlink_to('/tmp')
            with self.assertRaises(ValueError): m.safe_path(root,'link/escape')

    def test_replace_preserves_permissions(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); source=root/'input.apk'; dest=root/'base.apk'
            source.write_bytes(b'new'); dest.write_bytes(b'old'); dest.chmod(0o640)
            m.replace_file(source,dest)
            self.assertEqual(dest.read_bytes(),b'new')
            self.assertEqual(dest.stat().st_mode & 0o777,0o640)
            self.assertFalse((root/'base.apk.nexus-new').exists())

    def test_refuse_links_and_pending(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); source=root/'input'; source.write_bytes(b'new')
            dest=root/'output'; dest.symlink_to(source)
            with self.assertRaises(ValueError): m.replace_file(source,dest)
            dest.unlink(); (root/'output.nexus-new').write_bytes(b'pending')
            with self.assertRaises(FileExistsError): m.replace_file(source,dest)
            self.assertEqual(source.read_bytes(),b'new')

    def test_unsaved_close_refused(self):
        workspace=m.PortableWorkspace(); workspace.dirty=True
        with self.assertRaises(ValueError): workspace.close()

if __name__=='__main__': unittest.main()
