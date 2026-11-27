"""Read-only guest display/HAL inventory. Never patches the system image or loads a driver."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda:f.read(4194304),b''): h.update(b)
    return h.hexdigest()


def main():
    image=Path(sys.argv[1]);config=Path(sys.argv[2]).read_text()
    before=digest(image)
    values=dict(line[7:].split('=',1) for line in config.splitlines() if line.startswith('CONFIG_') and '=' in line)
    keys=['DRM','DRM_VIRTIO_GPU','DRM_BOCHS','DRM_VMWGFX','DRM_QXL','VIRTIO_MMIO','VIRTIO_PCI','FB','FB_VESA','FB_SIMPLE','SYSFB_SIMPLEFB']
    report={'schema':1,'inspection':'debugfs read-only, no -w, no mounts','image_sha256_before':before,
            'kernel_config':{k:values.get(k,'not set') for k in keys},'directories':{},'files':{},
            'android_graphics_verified':False,'guest_acceleration_verified':False}
    def read(command):
        p=subprocess.run(['debugfs','-R',command,str(image)],capture_output=True,text=True,timeout=30)
        if len(p.stdout)>1048576: raise ValueError('Guest audit output budget exceeded')
        return {'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
    directories=['/','/system','/vendor','/lib/modules','/system/lib/modules','/vendor/lib/modules']
    for prefix in ['', '/system']:
        for sub in ['/lib/egl','/lib64/egl','/lib/hw','/lib64/hw','/vendor/lib/egl','/vendor/lib64/egl','/vendor/lib/hw','/vendor/lib64/hw','/vendor/etc/init','/etc/init']:
            directories.append(prefix+sub)
    for directory in dict.fromkeys(directories): report['directories'][directory]=read('ls -p '+directory)
    for path in ['/build.prop','/system/build.prop','/vendor/build.prop','/system/vendor/build.prop','/init.rc','/init.android_x86.rc','/system/etc/init/hw/init.rc','/vendor/etc/init/hw/init.rc','/system/vendor/etc/init/hw/init.rc']:
        result=read('cat '+path)
        if path.endswith('build.prop'):
            result['stdout']='\n'.join(line for line in result['stdout'].splitlines() if any(k in line.lower() for k in ['egl','gralloc','hwcomposer','vulkan','gpu','opengl','hardware','debuggable','build.type']))
        report['files'][path]=result
    report['image_sha256_after']=digest(image)
    assert before==report['image_sha256_after'],'Read-only audit changed image'
    Path('native-graphics-guest-audit.json').write_text(json.dumps(report,indent=2))
    print('::notice::GUEST_GRAPHICS_KERNEL='+json.dumps(report['kernel_config']),flush=True)
    for section in ['directories','files']:
        for path,result in report[section].items():
            text=result['stdout']
            if text.strip():
                for at in range(0,len(text),2800):
                    msg=f'GUEST_GRAPHICS_{section}_{path}_{at}='+text[at:at+2800]
                    print('::notice::'+msg.replace('%','%25').replace('\r','%0D').replace('\n','%0A'),flush=True)
    assert any('egl' in r['stdout'].lower() or 'gralloc' in r['stdout'].lower() for r in report['directories'].values()),'No graphics library inventory found; inspect filesystem layout'


if __name__=='__main__': main()
