"""Boot the exact bundled Windows engine + appliance and edit a fixture copy."""
import base64
import hashlib
import importlib.util
from pathlib import Path
import shutil
import tempfile
spec=importlib.util.spec_from_file_location('host','portable/editor_host.py')
h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
source=Path('portable/appliance/test-fixture.qcow2').resolve()
before=hashlib.sha256(source.read_bytes()).hexdigest()
with tempfile.TemporaryDirectory() as d:
    try:
        print(h.open_copy(source,Path(d)/'edited.qcow2'),flush=True)
        assert h.guest({'op':'list','view':'system','path':'/system/app/OldLauncher'})['entries'][0]['name']=='base.apk'
        h.guest({'op':'import','view':'system','path':'/system/app/OldLauncher/base.apk','data':base64.b64encode(b'windows-bundled-runtime').decode()})
        assert base64.b64decode(h.guest({'op':'export','view':'system','path':'/system/app/OldLauncher/base.apk'})['data'])==b'windows-bundled-runtime'
        h.guest({'op':'save'});h.guest({'op':'close'})
    finally:
        if h.vm and h.vm.poll() is None:h.vm.terminate();h.vm.wait(timeout=30)
        if h.log:h.log.close()
        for log in Path(d).glob('*.nexus-work/maintenance.log'): shutil.copyfile(log,'windows-appliance-test.log')
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
source.unlink() # fixture is test-only, not shipped
print('PASS: exact bundled Windows Python/QEMU, TCG boot, nested system import/export/save, original SHA unchanged',flush=True)
