"""Exercise QMP input commands against a real QEMU process; not an Android test."""
import json
import os
import queue
import subprocess
import threading

firmware = ['-L', os.environ['QEMU_DATA']] if os.environ.get('QEMU_DATA') else []
args = [os.environ.get('QEMU_SYSTEM', 'qemu-system-x86_64'), *firmware, '-accel', 'tcg', '-m', '64',
        '-display', 'none', '-vga', 'none', '-device', 'VGA,id=nexus_display',
        '-device', 'virtio-multitouch-pci,id=nexus_touch,display=nexus_display', '-monitor', 'none', '-serial', 'none', '-nic', 'none', '-usb', '-device', 'usb-tablet', '-qmp', 'stdio']
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
    command('input-send-event', {'events': [
        {'type': 'abs', 'data': {'axis': 'x', 'value': 20000}},
        {'type': 'abs', 'data': {'axis': 'y', 'value': 18000}}]})
    command('input-send-event', {'events': [{'type': 'btn', 'data': {'button': 'left', 'down': False}}]})
    def mtt(kind, slot, axis='x', value=0, tracking=None):
        return {'type': 'mtt', 'data': {'type': kind, 'slot': slot,
                'tracking-id': slot if tracking is None else tracking, 'axis': axis, 'value': value}}
    touch_on = {'type': 'btn', 'data': {'button': 'touch', 'down': True}}
    touch_off = {'type': 'btn', 'data': {'button': 'touch', 'down': False}}
    # Two concurrent slots; QMP acceptance does not prove guest delivery.
    command('input-send-event', {'device': 'nexus_display', 'events': [touch_on, mtt('begin', 0), mtt('data', 0, 'x', 14000), mtt('data', 0, 'y', 15000)]})
    command('input-send-event', {'device': 'nexus_display', 'events': [mtt('begin', 1), mtt('data', 1, 'x', 20000), mtt('data', 1, 'y', 22000)]})
    command('input-send-event', {'device': 'nexus_display', 'events': [mtt('update', 0), mtt('data', 0, 'x', 16000), mtt('data', 0, 'y', 17000)]})
    command('input-send-event', {'device': 'nexus_display', 'events': [mtt('end', 1, tracking=-1)]})
    command('input-send-event', {'device': 'nexus_display', 'events': [mtt('end', 0, tracking=-1), touch_off]})
    command('quit')
    p.wait(timeout=10)
    print('QMP handshake and single-pointer and two-slot QMP commands accepted (guest input not verified).')
finally:
    if p.poll() is None:
        p.kill(); p.wait()
