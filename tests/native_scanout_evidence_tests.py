import base64
import unittest
from native_scanout_evidence import scanout_evidence

class ScanoutEvidenceTests(unittest.TestCase):
    def test_uniform_capture_is_not_home(self):
        d=scanout_evidence(b'P6\n2 2\n255\n'+bytes(12))
        self.assertFalse(d['nonuniform']);self.assertEqual(d['dominant_color_fraction'],1)
        self.assertFalse(d['android_home_verified'])
        self.assertTrue(base64.b64decode(d['preview_png_base64']).startswith(b'\x89PNG\r\n\x1a\n'))
    def test_pattern_is_not_home(self):
        d=scanout_evidence(b'P6\n2 1\n255\n'+bytes([255,0,0,0,255,0]))
        self.assertTrue(d['nonuniform']);self.assertEqual(d['unique_rgb_colors'],2)
        self.assertFalse(d['android_home_verified'])
    def test_invalid_captures_rejected(self):
        for data in (b'',b'P6\n1 1\n255\n',b'P6\n0 1\n255\n',b'P3\n1 1\n255\n123'):
            with self.assertRaises(ValueError):scanout_evidence(data)

if __name__=='__main__':unittest.main()
