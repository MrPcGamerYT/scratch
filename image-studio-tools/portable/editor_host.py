"""Portable Windows/browser controller. Uses only bundled Python and QEMU."""
import base64
import ctypes
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.request
import webbrowser

ROOT=Path(__file__).resolve().parent
TOKEN=secrets.token_hex(32)
vm=None
log=None
work=None
guest_port=None
closed=False

def command(args, **kwargs):
    result=subprocess.run([str(x) for x in args],capture_output=True,text=True,**kwargs)
    if result.returncode: raise RuntimeError(f'{Path(str(args[0])).name}: '+result.stderr[-2000:])
    return result.stdout

def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1',0)); return s.getsockname()[1]

def guest(data,timeout=900):
    request=urllib.request.Request(f'http://127.0.0.1:{guest_port}/',data=json.dumps(data).encode(),headers={'Authorization':'Bearer '+TOKEN,'Content-Type':'application/json'})
    with urllib.request.urlopen(request,timeout=timeout) as response: result=json.load(response)
    if not result.get('ok'): raise ValueError(result.get('error','Maintenance VM error'))
    return result

def pick(save=False):
    if sys.platform!='win32': raise ValueError('Native picker requires Windows; enter a full path instead')
    from ctypes import wintypes
    class OFN(ctypes.Structure):
        _fields_=[('lStructSize',wintypes.DWORD),('hwndOwner',wintypes.HWND),('hInstance',wintypes.HINSTANCE),('lpstrFilter',wintypes.LPCWSTR),('lpstrCustomFilter',wintypes.LPWSTR),('nMaxCustFilter',wintypes.DWORD),('nFilterIndex',wintypes.DWORD),('lpstrFile',wintypes.LPWSTR),('nMaxFile',wintypes.DWORD),('lpstrFileTitle',wintypes.LPWSTR),('nMaxFileTitle',wintypes.DWORD),('lpstrInitialDir',wintypes.LPCWSTR),('lpstrTitle',wintypes.LPCWSTR),('Flags',wintypes.DWORD),('nFileOffset',wintypes.WORD),('nFileExtension',wintypes.WORD),('lpstrDefExt',wintypes.LPCWSTR),('lCustData',wintypes.LPARAM),('lpfnHook',ctypes.c_void_p),('lpTemplateName',wintypes.LPCWSTR),('pvReserved',ctypes.c_void_p),('dwReserved',wintypes.DWORD),('FlagsEx',wintypes.DWORD)]
    buf=ctypes.create_unicode_buffer(32768); config=OFN(); config.lStructSize=ctypes.sizeof(config)
    config.lpstrFile=ctypes.cast(buf,wintypes.LPWSTR); config.nMaxFile=len(buf)
    config.lpstrFilter='Guest images\0*.qcow2;*.img;*.raw\0All files\0*.*\0\0'
    config.lpstrTitle='New working copy (must not exist)' if save else 'Stopped source guest image'
    config.Flags=0x00080000|0x00000008|(0x00001000 if not save else 0)
    config.lpstrDefExt='qcow2'
    method=ctypes.windll.comdlg32.GetSaveFileNameW if save else ctypes.windll.comdlg32.GetOpenFileNameW
    return buf.value if method(ctypes.byref(config)) else ''

def open_copy(source,destination):
    global vm,log,work,guest_port
    if vm is not None: raise ValueError('Restart the editor before opening another image')
    source=Path(source).resolve(strict=True); destination=Path(destination).absolute()
    if destination.suffix.lower()!='.qcow2': raise ValueError('New copy must end in .qcow2')
    if source==destination or destination.exists(): raise ValueError('Choose a NEW destination. Nothing existing will be overwritten.')
    qemu=ROOT/'qemu/bin'/('NexusEngine.exe' if sys.platform=='win32' else 'qemu-system-x86_64')
    img=ROOT/'qemu/bin'/('qemu-img.exe' if sys.platform=='win32' else 'qemu-img')
    info=json.loads(command([img,'info','--output=json',source],timeout=60))
    if info.get('format') not in ('qcow2','raw') or info.get('backing-filename') or info.get('encrypted'):
        raise ValueError('Only standalone, unencrypted raw/qcow2 sources are supported')
    if shutil.disk_usage(destination.parent).free < info['virtual-size']+4*1024**3:
        raise ValueError('Keep at least guest virtual size + 4 GiB free for copy and recovery')
    # Exclusive reserve prevents accidental destination overwrite. qemu-img takes
    # its normal source lock; do not pass -U/force-share or bypass in-use checks.
    with destination.open('xb'): pass
    work=destination.with_name(destination.name+'.nexus-work'); work.mkdir()
    try:
        command([img,'convert','-f',info['format'],'-O','qcow2',source,destination],timeout=3600)
        command([img,'check',destination],timeout=600)
        command([img,'create','-f','qcow2',work/'scratch.qcow2','16G'],timeout=60)
        guest_port=port()
        log=(work/'maintenance.log').open('w')
        args=[qemu,'-L',ROOT/'qemu/share/qemu','-machine','q35','-accel','tcg','-m','1536','-smp','2','-display','none','-monitor','none','-serial','stdio','-no-reboot',
              '-kernel',ROOT/'appliance/vmlinuz','-initrd',ROOT/'appliance/initrd',
              '-append',f'console=ttyS0 net.ifnames=0 panic=-1 nexus_token={TOKEN}',
              '-drive',f'file={destination},format=qcow2,if=virtio',
              '-drive',f'file={work / "scratch.qcow2"},format=qcow2,if=virtio',
              '-netdev',f'user,id=api,restrict=on,hostfwd=tcp:127.0.0.1:{guest_port}-:8080','-device','virtio-net-pci,netdev=api,romfile=']
        # QEMU's -drive parser uses a doubled comma for literal commas in paths.
        args=[str(a) for a in args]
        for i,a in enumerate(args):
            if i and args[i-1]=='-drive':
                filepart,rest=a.split(',format=',1); args[i]=filepart.replace(',',',,')+',format='+rest
        vm=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT)
        deadline=time.monotonic()+420
        while time.monotonic()<deadline:
            if vm.poll() is not None: raise ValueError('Maintenance VM exited. See '+str(work/'maintenance.log'))
            try: guest({'op':'hello'},timeout=2); break
            except Exception: time.sleep(2)
        else: raise TimeoutError('Maintenance VM startup timed out; retain working folder and log')
        guest({'op':'open'})
        return {'ok':True,'destination':str(destination)}
    except Exception:
        # Never delete recovery output, even if initialization failed.
        if vm and vm.poll() is None: vm.terminate(); vm.wait(timeout=30)
        raise

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path!='/': self.send_error(404); return
        body=(ROOT/'editor.html').read_text().replace('__TOKEN__',TOKEN).encode()
        self.send_response(200); self.send_header('Content-Type','text/html; charset=utf-8'); self.send_header('Content-Length',str(len(body))); self.send_header('Cache-Control','no-store'); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        global closed
        try:
            # Token + exact local Origin stops browser-based localhost attacks.
            origin=f'http://127.0.0.1:{self.server.server_port}'
            if self.headers.get('X-Nexus-Token')!=TOKEN or self.headers.get('Origin')!=origin: raise ValueError('Unauthorized local request')
            length=int(self.headers.get('Content-Length',0))
            if not 0<length<=96*1024**2: raise ValueError('Invalid request size')
            data=json.loads(self.rfile.read(length)); op=data['op']
            if op=='pick': result={'ok':True,'path':pick(data.get('save',False))}
            elif op=='open': result=open_copy(data['source'],data['destination'])
            elif op=='finish':
                guest({'op':'save'}); guest({'op':'close'})
                if vm and vm.poll() is None: vm.terminate(); vm.wait(timeout=30)
                closed=True; result={'ok':True}
            else:
                if vm is None or closed: raise ValueError('Open a working copy first')
                result=guest(data)
        except Exception as exc: result={'ok':False,'error':str(exc)}
        payload=json.dumps(result).encode(); self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(payload))); self.end_headers(); self.wfile.write(payload)

def main():
    server=HTTPServer(('127.0.0.1',0),Handler)
    url=f'http://127.0.0.1:{server.server_port}'
    print('Nexus Portable Guest Editor\n'+url+'\nKeep this window open. Use Save & Finish before closing. Ctrl+C exits.')
    threading.Timer(1,lambda:webbrowser.open(url)).start()
    try: server.serve_forever()
    except KeyboardInterrupt: pass
    finally:
        if vm and vm.poll() is None:
            print('Stopping maintenance VM. Unfinished files retained in',work)
            vm.terminate(); vm.wait(timeout=30)
        if log: log.close()

if __name__=='__main__': main()
