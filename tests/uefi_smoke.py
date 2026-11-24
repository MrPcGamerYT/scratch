"""Launch bundled UEFI firmware under TCG; does not test Android or WHPX."""
import hashlib
import json
import os
import pathlib
import queue
import shutil
import subprocess
import tempfile
import threading
import time

code = pathlib.Path(os.environ['QEMU_DATA']) / 'edk2-x86_64-code.fd'
template = pathlib.Path(os.environ['QEMU_DATA']) / 'edk2-i386-vars.fd'
with tempfile.TemporaryDirectory(prefix='uefi,test ') as d:
    root = pathlib.Path(d); variables = root/'vars, instance.fd'
    shutil.copyfile(template, variables)
    original = hashlib.sha256(template.read_bytes()).digest()
    def drive(path): return str(path).replace(',', ',,')
    args = [os.environ['QEMU_SYSTEM'], '-L', os.environ['QEMU_DATA'], '-machine', 'q35,vmport=off',
            '-accel', 'tcg,thread=multi', '-cpu', 'max', '-smp', '2', '-m', '256', '-display', 'none', '-monitor', 'none', '-serial', 'none', '-nic', 'none',
            '-drive', f'if=pflash,format=raw,unit=0,readonly=on,file={drive(code)}',
            '-drive', f'if=pflash,format=raw,unit=1,file={drive(variables)}', '-qmp', 'stdio']
    p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    lines = queue.Queue()
    def reader():
        for line in p.stdout: lines.put(line)
        lines.put(None)
    threading.Thread(target=reader, daemon=True).start()
    def receive():
        while True:
            line = lines.get(timeout=30)
            if line is None: raise RuntimeError('UEFI QEMU exited: ' + p.stderr.read())
            try: return json.loads(line)
            except json.JSONDecodeError: continue
    def cmd(name, arguments=None):
        msg = {'execute': name, 'id': name}
        if arguments is not None: msg['arguments'] = arguments
        p.stdin.write(json.dumps(msg)+'\n'); p.stdin.flush()
        while True:
            reply = receive()
            if reply.get('id') == name:
                if 'error' in reply: raise RuntimeError(str(reply['error']))
                return reply
    try:
        assert 'QMP' in receive(); cmd('qmp_capabilities')
        time.sleep(8)
        assert cmd('query-status')['return']['running']
        screenshot = root/'screen.ppm'; cmd('screendump', {'filename': str(screenshot)})
        data = screenshot.read_bytes(); idx = data.find(b'255\n')
        if idx < 0 or not any(data[idx+4:]): raise RuntimeError('UEFI did not display visible output in the smoke-test window')
        cmd('quit'); p.wait(timeout=15)
        assert p.returncode == 0 and hashlib.sha256(template.read_bytes()).digest() == original
        print('Q35 UEFI firmware displayed output with isolated variable store (explicit multi-thread TCG/max CPU, no Android or WHPX).')
    finally:
        if p.poll() is None: p.kill(); p.wait()
