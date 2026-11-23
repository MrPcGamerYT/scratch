"""Exercise QMP input commands against a real QEMU process; not an Android test."""
import json
import os
import queue
import subprocess
import threading

firmware = ['-L', os.environ['QEMU_DATA']] if os.environ.get('QEMU_DATA') else []
args = [os.environ.get('QEMU_SYSTEM', 'qemu-system-x86_64'), *firmware, '-accel', 'tcg', '-m', '64',
        '-display', 'none', '-monitor', 'none', '-serial', 'none', '-nic', 'none', '-usb', '-device', 'usb-tablet', '-qmp', 'stdio']
p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
lines = queue.Queue()
def reader():
    for line in p.stdout:
        lines.put(line)
    lines.put(None)
threading.Thread(target=reader, daemon=True).start()
def receive():
    while True:
        line = lines.get(timeout=20)
        if line is None:
            raise RuntimeError('QEMU exited: ' + p.stderr.read())
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
def command(name, arguments=None):
    request = {'execute': name, 'id': name}
    if arguments is not None:
        request['arguments'] = arguments
    p.stdin.write(json.dumps(request) + '\n'); p.stdin.flush()
    while True:
        response = receive()
        if response.get('id') == name:
            if 'error' in response:
                raise RuntimeError(str(response['error']))
            return response
try:
    assert 'QMP' in receive()
    command('qmp_capabilities')
    command('input-send-event', {'events': [
        {'type': 'abs', 'data': {'axis': 'x', 'value': 16384}},
        {'type': 'abs', 'data': {'axis': 'y', 'value': 16384}},
        {'type': 'btn', 'data': {'button': 'left', 'down': True}}]})
    command('input-send-event', {'events': [{'type': 'btn', 'data': {'button': 'left', 'down': False}}]})
    command('quit')
    p.wait(timeout=10)
    print('QMP handshake and single-pointer press/release succeeded.')
finally:
    if p.poll() is None:
        p.kill(); p.wait()
