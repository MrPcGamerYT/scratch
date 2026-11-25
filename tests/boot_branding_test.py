import gzip
from pathlib import Path
import stat
import sys
import unittest
sys.path.insert(0,str(Path('guest-tools').resolve()))
from install_bridge_ramdisk import records,record
from rebrand_boot import patch,patch_script
class BrandingTests(unittest.TestCase):
    def test_bounded_idempotent(self):
        header=b'# Modified by BlissLabs & Contributors\n# License: GNU Public License\n'
        banner=b'text_art()\n{\ncat << "EOF"\nASCII Bliss logo\nHave A Truly Blissful Experience\nEOF\n}'
        suffix=b'\nro.product.bliss.name=keep\n'
        script=header+banner+suffix
        fields=[1,stat.S_IFREG|0o755,0,0,1,123,0,0,0,0,0,0,0]
        raw=record('init',fields,script)+record('untouched',fields,b'Bliss licence')+record('TRAILER!!!',[0]*13,b'')
        updated=patch(gzip.compress(raw))
        items=list(records(gzip.decompress(updated)))
        self.assertTrue(items[0][2].startswith(header));self.assertTrue(items[0][2].endswith(suffix))
        self.assertIn(b'Welcome to Nexus Android',items[0][2]);self.assertEqual(items[0][1][1],fields[1])
        self.assertEqual(items[1][2],b'Bliss licence')
        self.assertEqual(patch_script(items[0][2]),items[0][2])
    def test_unknown_refused(self):
        with self.assertRaises(ValueError):patch_script(b'echo Welcome to other OS')
if __name__=='__main__':unittest.main()
