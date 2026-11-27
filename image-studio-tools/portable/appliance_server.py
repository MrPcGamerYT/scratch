"""Private maintenance VM API. No guest executable is ever run."""
import base64
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import sys
import tempfile
import traceback
sys.path.insert(0,'/opt/nexus')
from portable_workspace import PortableWorkspace, safe_path, replace_file, run

TOKEN=next((x.split('=',1)[1] for x in Path('/proc/cmdline').read_text().split() if x.startswith('nexus_token=')),None)
workspace=PortableWorkspace()

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        try:
            if not TOKEN or self.headers.get('Authorization')!='Bearer '+TOKEN: raise ValueError('Unauthorized')
            length=int(self.headers.get('Content-Length',0))
            if not 0<length<=96*1024*1024: raise ValueError('Request size invalid (64 MiB files maximum)')
            data=json.loads(self.rfile.read(length))
            op=data['op']; result={}
            if op=='hello': result={'ready':True}
            elif op=='open': workspace.mount(); result={'ready':True}
            elif op=='brand':
                from rebrand_boot import patch
                if workspace.base is None: raise ValueError('Open a disk first')
                initrd=safe_path(workspace.base,'initrd.img')
                if initrd.stat().st_size>32*1024**2: raise ValueError('Unsupported ramdisk size')
                updated=patch(initrd.read_bytes())
                with tempfile.TemporaryDirectory(dir=workspace.scratch) as folder:
                    source=Path(folder)/'initrd.img'; source.write_bytes(updated)
                    workspace.dirty=True; replace_file(source,initrd)
                result={'message':'Nexus startup banner installed. Upstream notices retained.'}
            elif op=='save': workspace.save()
            elif op=='close': workspace.close()
            else:
                root=workspace.open_system() if data.get('view')=='system' else workspace.base
                if root is None: raise ValueError('Open a disk first')
                path=safe_path(root,data.get('path','/'))
                if op=='list':
                    if not path.is_dir(): raise ValueError('Not a directory')
                    result={'entries':[{'name':p.name,'link':p.is_symlink(),'directory':p.is_dir() and not p.is_symlink(),'size':p.lstat().st_size} for p in sorted(path.iterdir(),key=lambda p:p.name)][:10000]}
                elif op=='export':
                    if not path.is_file() or path.stat().st_size>64*1024**2: raise ValueError('Choose a regular file up to 64 MiB')
                    result={'data':base64.b64encode(path.read_bytes()).decode()}
                elif op in ('import','apk'):
                    content=base64.b64decode(data['data'],validate=True)
                    if len(content)>64*1024**2: raise ValueError('64 MiB file limit')
                    with tempfile.TemporaryDirectory(dir=workspace.scratch) as folder:
                        source=Path(folder)/('import.apk' if op=='apk' else 'import.bin'); source.write_bytes(content)
                        if op=='apk': result={'package':workspace.add_apk(source)}
                        else:
                            if not path.parent.is_dir(): raise ValueError('Create parent directory first')
                            if path.name in ('system.sfs','system.img') and workspace.system in workspace.mounts: raise ValueError('Cannot replace mounted system payload')
                            workspace.dirty=True
                            replace_file(source,path,new_system_file=data.get('view')=='system')
                elif op=='mkdir':
                    path.mkdir(mode=0o755)
                    if data.get('view')=='system': os.setxattr(path,'security.selinux',b'u:object_r:system_file:s0')
                    workspace.dirty=True
                else: raise ValueError('Unsupported operation')
            payload=json.dumps({'ok':True,**result}).encode()
        except Exception as exc:
            traceback.print_exc()
            payload=json.dumps({'ok':False,'error':str(exc)}).encode()
        self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(payload))); self.end_headers(); self.wfile.write(payload)

if __name__=='__main__':
    # Never format any disk except the explicitly provided disposable vdb.
    run('mkfs.ext4','-F','-L','NEXUS_SCRATCH','/dev/vdb')
    HTTPServer(('0.0.0.0',8080),Handler).serve_forever()
