"""Run production framing/workers against explicitly compiled sandbox utilities.
No Android/root privileges: not a substitute for the actual Android image gate.
"""
import hashlib
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]


class NativeGuestIntegrationTests(unittest.TestCase):
    def test_production_com2_raw_mode_via_explicit_test_pty(self):
        import pty
        import termios
        master,slave=pty.openpty()
        process=None
        try:
            with tempfile.TemporaryDirectory(prefix='nexus-com2-') as directory:
                root=Path(directory);utilities=root/'bin';utilities.mkdir()
                for name,body in {'getprop':'echo 0','dumpsys':"echo 'topResumedActivity=ActivityRecord{abc u0 org.example.game/.Main t1}'"}.items():
                    path=utilities/name;path.write_text('#!/bin/sh\n'+body+'\n');path.chmod(0o755)
                executable=root/'agent'
                subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror','-DNEXUS_BRIDGE_COM2','-DNEXUS_AGENT_COM2_TEST',
                                f'-DNEXUS_COM2_TEST_PATH="{os.ttyname(slave)}"',f'-DNEXUS_SYSTEM_BIN="{utilities}/"',
                                str(ROOT/'guest-tools/nexus-guest/agent.c'),'-o',str(executable)],check=True)
                process=subprocess.Popen([str(executable)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
                pending=bytearray()
                def line():
                    deadline=time.monotonic()+8
                    while b'\n' not in pending:
                        budget=deadline-time.monotonic()
                        self.assertGreater(budget,0,'COM2 daemon reply timed out')
                        self.assertTrue(select.select([master],[],[],budget)[0],'COM2 daemon reply timed out')
                        pending.extend(os.read(master,4096))
                    at=pending.index(10);result=bytes(pending[:at]);del pending[:at+1];return result
                self.assertEqual(line(),b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK')
                settings=termios.tcgetattr(slave)
                self.assertEqual(settings[0],0);self.assertEqual(settings[1],0);self.assertEqual(settings[3],0)
                os.write(master,b'NEXUS 1 QUERY_APP\n')
                frames=[]
                for _ in range(5):
                    frames.append(line())
                    if frames[-1]==b'NEXUS 1 RESULT QUERY_APP OK': break
                self.assertIn(b'NEXUS 1 APP org.example.game',frames)
                self.assertEqual(frames[-1],b'NEXUS 1 RESULT QUERY_APP OK')
                self.assertNotIn(b'NEXUS 1 QUERY_APP',frames)  # no terminal echo into protocol
        finally:
            if process is not None:
                process.terminate();process.wait(timeout=5);process.stderr.close()
            os.close(master);os.close(slave)


    def test_binary_apk_integrity_async_heartbeat_and_disconnect_cleanup(self):
        with tempfile.TemporaryDirectory(prefix='nexus-ci-') as directory:
            root = Path(directory)
            utilities = root/'bin'
            utilities.mkdir()
            for name, body in {
                'getprop': 'echo 1',
                'input': 'sleep 3',
                'dumpsys': "echo 'topResumedActivity=ActivityRecord{abc u0 org.example.game/.Main t1}'",
                'pm': f'cat > "{root}/installed.apk"',
                'svc': 'exit 0',
            }.items():
                path = utilities/name
                path.write_text('#!/bin/sh\n'+body+'\n')
                path.chmod(0o755)
            (utilities/'sha256sum').symlink_to('/usr/bin/sha256sum')
            executable = root/'agent'
            subprocess.run(['cc','-std=c11','-Wall','-Wextra','-Werror',
                            '-DNEXUS_AGENT_HARNESS', f'-DNEXUS_SYSTEM_BIN="{utilities}/"',
                            f'-DNEXUS_APK_DIR="{root}/"',
                            str(ROOT/'guest-tools/nexus-guest/agent.c'),'-o',str(executable)],check=True)
            process = subprocess.Popen([str(executable)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0)
            pending = bytearray()
            def line(timeout=12):
                deadline=time.monotonic()+timeout
                while b'\n' not in pending:
                    remaining=deadline-time.monotonic()
                    if remaining<=0 or not select.select([process.stdout],[],[],remaining)[0]:
                        self.fail('Native reply timed out: '+repr(pending))
                    data=os.read(process.stdout.fileno(),4096)
                    if not data: self.fail('Native agent exited: '+process.stderr.read().decode())
                    pending.extend(data)
                value,_,tail=pending.partition(b'\n')
                pending[:]=tail
                return bytes(value)
            def expect(expected):
                while True:
                    value=line()
                    if value.startswith(b'NEXUS 1 READY '): continue
                    self.assertEqual(value,expected)
                    return
            def send(data):
                process.stdin.write(data)
                process.stdin.flush()
            try:
                expect(b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK')
                send(b'NEXUS 1 HOME\n')
                heartbeats=0
                while True:
                    value=line()
                    if value==b'NEXUS 1 RESULT HOME OK': break
                    self.assertEqual(value,b'NEXUS 1 READY 1')
                    heartbeats+=1
                self.assertGreaterEqual(heartbeats,1,'Worker must not starve readiness heartbeat')
                time.sleep(.1)
                send(b'NEXUS 1 QUERY_APP\n')
                expect(b'NEXUS 1 APP org.example.game')
                expect(b'NEXUS 1 RESULT QUERY_APP OK')
                time.sleep(.1)
                data=bytes(range(256))*600  # Binary includes NUL, LF and every non-ASCII byte.
                def upload(digest):
                    send(b'NEXUS 1 APK BEGIN '+str(len(data)).encode()+b' '+digest.encode()+b'\n')
                    expect(b'NEXUS 1 APK NEXT')
                    for offset in range(0,len(data),65536):
                        chunk=data[offset:offset+65536]
                        send(b'NEXUS 1 APK DATA '+str(len(chunk)).encode()+b'\n'+chunk)
                        expect(b'NEXUS 1 APK NEXT')
                    send(b'NEXUS 1 APK COMMIT\n')
                upload('0'*64)
                expect(b'NEXUS 1 RESULT APK_INSTALL FAILED')
                self.assertFalse((root/'installed.apk').exists(),'Digest mismatch must never reach installer')
                time.sleep(.1)
                upload(hashlib.sha256(data).hexdigest())
                expect(b'NEXUS 1 RESULT APK_INSTALL OK')
                self.assertEqual((root/'installed.apk').read_bytes(),data)
                time.sleep(.1)
                send(b'NEXUS 1 APK BEGIN 100 '+b'0'*64+b'\n')
                expect(b'NEXUS 1 APK NEXT')
                send(b'NEXUS 1 APK DATA 100\npartial')
                process.stdin.close()
                process.wait(timeout=5)
                self.assertFalse(list(root.glob('nexus-install-*.apk')),'Disconnect must remove partial staging files')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                process.stdout.close()
                process.stderr.close()
                if not process.stdin.closed: process.stdin.close()


if __name__ == '__main__':
    unittest.main()
