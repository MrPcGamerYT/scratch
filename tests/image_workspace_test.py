"""Real libguestfs offline copy and filesystem test, not an Android boot test."""
import importlib.util
import os
import pathlib
import subprocess
import tempfile

os.environ['LIBGUESTFS_BACKEND'] = 'direct'
os.environ['LIBGUESTFS_BACKEND_SETTINGS'] = 'force_tcg'
import guestfs

spec = importlib.util.spec_from_file_location('image_workspace', 'image-studio-tools/image_workspace.py')
mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
with tempfile.TemporaryDirectory() as d:
    root = pathlib.Path(d)
    original, work = root/'original.qcow2', root/'working.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', str(original), '128M'], check=True)
    g = guestfs.GuestFS(python_return_dict=True)
    try:
        g.add_drive_opts(str(original), format='qcow2', readonly=False); g.launch()
        g.part_disk('/dev/sda', 'mbr'); g.mkfs('ext4', '/dev/sda1'); g.mount('/dev/sda1', '/')
        g.write('/original.txt', b'original contents'); g.umount_all(); g.shutdown()
    finally: g.close()
    raw, raw_copy = root/'raw-source.img', root/'raw-copy.qcow2'
    subprocess.run(['qemu-img', 'convert', '-f', 'qcow2', '-O', 'raw', str(original), str(raw)], check=True)
    mod.create_copy(str(raw), str(raw_copy))
    subprocess.run(['qemu-img', 'compare', '-f', 'raw', '-F', 'qcow2', str(raw), str(raw_copy)], check=True)
    mod.create_copy(str(original), str(work))
    try: mod.create_copy(str(original), str(work)); raise AssertionError('Overwrite accepted')
    except FileExistsError: pass
    bad = root/'backed.qcow2'
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'qcow2', '-b', str(original), str(bad)], check=True)
    try: mod.create_copy(str(bad), str(root/'badcopy.qcow2')); raise AssertionError('Backing file accepted')
    except ValueError: pass
    try: mod.child_path('/', '../escape'); raise AssertionError('Path escape accepted')
    except ValueError: pass
    w = mod.Workspace(str(work))
    try:
        w.mount('/dev/sda1')
        assert ('original.txt', False) in w.entries()
        src = root/'host.txt'; src.write_text('changed')
        w.upload_file(str(src), 'original.txt')
        w.mkdir('apps'); w.enter('apps'); w.upload_file(str(src), 'new.txt'); w.up()
        out = root/'export.txt'; w.export_file('original.txt', str(out)); assert out.read_text() == 'changed'
        try: w.export_file('original.txt', str(out)); raise AssertionError('Export overwrote a host file')
        except FileExistsError: pass
        w.delete_file('original.txt'); assert not w.exists('original.txt')
    finally: w.close()
    g = guestfs.GuestFS(python_return_dict=True)
    try:
        g.add_drive_opts(str(original), format='qcow2', readonly=True); g.launch(); g.mount_ro('/dev/sda1', '/')
        original_export = root/'original-export.txt'; g.download('/original.txt', str(original_export))
        assert original_export.read_text() == 'original contents'
        assert not g.exists('/apps')
        g.umount_all(); g.shutdown()
    finally: g.close()
print('Offline copy, filesystem operations and original preservation passed (not Android).')
