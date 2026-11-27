"""Exercise the actual native parser with privileged execution stubbed out."""
import pathlib
import os
import sys
import gzip
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/"guest-tools"))
from install_bridge_ramdisk import patch, record, records
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


class GuestBridgeTests(unittest.TestCase):
    def test_new_engine_com2_production_build(self):
        with tempfile.TemporaryDirectory() as folder:
            binary=pathlib.Path(folder)/'agent-com2'
            subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror','-DNEXUS_BRIDGE_COM2',str(ROOT/'guest-tools/nexus-guest/agent.c'),'-o',str(binary)],check=True)


    def test_ramdisk_patch_preserves_original_metadata(self):
        fields=[7,0o100644,0,0,1,1234,0,0,0,0,0,0,0]
        original=record('init.rc',fields,b'on boot\n    hostname android\n')+record('keep',fields,b'unchanged')+record('TRAILER!!!',[0]*13,b'')
        result=list(records(gzip.decompress(patch(gzip.compress(original),b'ELF-fixture'))))
        data={name:(metadata,body) for name,metadata,body in result}
        self.assertEqual(data['keep'][1],b'unchanged')
        self.assertEqual(data['keep'][0][5],1234)
        self.assertIn(b'import /init.nexus.rc',data['init.rc'][1])
        self.assertIn(b'/data/.nexus-provision',data['nexus-provision-home.sh'][1])
        self.assertNotIn('nexus-provision.sh',data)
        home=data['nexus-home-check.sh'][1]
        self.assertNotIn(b'am start -W',home)
        self.assertIn(b'timeout 15 am start',home)
        self.assertIn(b'timeout 10 dumpsys activity lastanr',home)
        self.assertEqual(data['nexus-agent'][0][1],0o100755)
        with self.assertRaises(ValueError): patch(b'not-cpio',b'agent')


    def test_foreground_parser_and_rejected_apk_headers(self):
        with tempfile.TemporaryDirectory() as folder:
            binary=pathlib.Path(folder)/'agent'
            subprocess.run(['cc','-std=c11','-Wall','-Wextra','-Werror','-DNEXUS_AGENT_TEST',str(ROOT/'guest-tools/nexus-guest/agent.c'),'-o',str(binary)],check=True)
            for fixture,expected in [('topResumedActivity=ActivityRecord{abc u0 org.example.game/.Main t1}',b'org.example.game'),('mResumedActivity: unknown',b'-')]:
                env=dict(os.environ,NEXUS_TEST_ACTIVITY=fixture)
                result=subprocess.run([str(binary)],input=b'NEXUS 1 QUERY_APP\nNEXUS 1 APK BEGIN 9999999999 bad\n',capture_output=True,env=env,timeout=5,check=True)
                self.assertIn(b'NEXUS 1 APP '+expected+b'\n',result.stdout)
                self.assertIn(b'NEXUS 1 APK FAILED\n',result.stdout)

    def test_bounded_whitelisted_protocol(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = pathlib.Path(folder) / 'agent'
            subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-DNEXUS_AGENT_TEST', str(ROOT/'guest-tools/nexus-guest/agent.c'), '-o', str(binary)], check=True)
            payload = b'NEXUS 1 HOME\nNEXUS 1 SHUTDOWN\nNEXUS 2 HOME\nNEXUS 1 HOME;reboot\n' + b'x'*1000 + b'\nNEXUS 1 BACK\nNEXUS 1 HO\x00ME\n'
            result = subprocess.run([str(binary)], input=payload, capture_output=True, timeout=5, check=True)
            self.assertEqual(result.stdout.splitlines(), [
                b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK', b'NEXUS 1 RESULT HOME OK',
                b'NEXUS 1 RESULT SHUTDOWN OK', b'NEXUS 1 ERROR UNSUPPORTED',
                b'NEXUS 1 ERROR UNSUPPORTED', b'NEXUS 1 ERROR OVERSIZE',
                b'NEXUS 1 RESULT BACK OK', b'NEXUS 1 ERROR OVERSIZE'])


if __name__ == '__main__':
    unittest.main()
