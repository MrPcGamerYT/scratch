"""Offline QCOW2 workspace. All writes go to a newly-created image copy."""
import json
import os
import posixpath
import shutil
import subprocess


def create_copy(source, destination, qemu_img='qemu-img'):
    source = os.path.abspath(source)
    destination = os.path.abspath(destination)
    if not os.path.isfile(source):
        raise ValueError('Source must be a regular QCOW2 file.')
    result = subprocess.run([qemu_img, 'info', '--output=json', source],
                            capture_output=True, text=True, check=True)
    info = json.loads(result.stdout)
    if info.get('format') not in ('qcow2', 'raw') or 'backing-filename' in info:
        raise ValueError('Only standalone QCOW2 or raw partition/disk images are supported.')
    # Exclusive creation avoids overwriting either the source or an existing image.
    # A failed copy is retained for inspection rather than deleting user data.
    if info['format'] == 'raw':
        with open(destination, 'xb'):
            pass  # Reserve exclusively before qemu-img writes its own output.
        subprocess.run([qemu_img, 'convert', '-f', 'raw', '-O', 'qcow2', source, destination], check=True)
    else:
        with open(source, 'rb') as inp, open(destination, 'xb') as out:
            shutil.copyfileobj(inp, out, 1024 * 1024)
    return destination


def child_path(directory, name):
    if (not directory.startswith('/') or not name or name in ('.', '..') or
            '/' in name or '\x00' in name or '\n' in name or '\r' in name):
        raise ValueError('Invalid guest filename.')
    return posixpath.join(directory, name)


class Workspace:
    def __init__(self, image, guestfs_module=None):
        if guestfs_module is None:
            import guestfs as guestfs_module
        self.g = guestfs_module.GuestFS(python_return_dict=True)
        self.mounted = False
        self.current = '/'
        try:
            self.g.add_drive_opts(image, format='qcow2', readonly=False)
            self.g.launch()
        except BaseException:
            self.g.close()
            raise

    def filesystems(self):
        return self.g.list_filesystems()

    def mount(self, device):
        if device not in self.filesystems():
            raise ValueError('Select a filesystem listed in this image.')
        if self.mounted:
            self.g.umount_all()
            self.mounted = False
        self.g.mount(device, '/')
        self.mounted = True
        self.current = '/'

    def entries(self):
        if not self.mounted:
            raise ValueError('Choose a filesystem first.')
        names = self.g.ls(self.current)
        return sorted([(n, self.g.is_dir(child_path(self.current, n))) for n in names],
                      key=lambda e: (not e[1], e[0].lower()))

    def enter(self, name):
        path = child_path(self.current, name)
        if not self.g.is_dir(path):
            raise ValueError('Not a directory.')
        self.current = path

    def up(self):
        self.current = posixpath.dirname(self.current.rstrip('/')) or '/'

    def export_file(self, name, target):
        path = child_path(self.current, name)
        if self.g.is_dir(path):
            raise ValueError('Select a file, not a directory.')
        # download does not provide exclusive creation, so reserve first.
        with open(target, 'xb'):
            pass
        self.g.download(path, target)

    def upload_file(self, source, name):
        path = child_path(self.current, name)
        if self.g.is_dir(path):
            raise ValueError('Cannot replace a directory with a file.')
        self.g.upload(source, path)

    def exists(self, name):
        return self.g.exists(child_path(self.current, name))

    def mkdir(self, name):
        path = child_path(self.current, name)
        if self.g.exists(path):
            raise FileExistsError('That guest path already exists.')
        self.g.mkdir(path)

    def delete_file(self, name):
        path = child_path(self.current, name)
        if self.g.is_dir(path):
            raise ValueError('Directory deletion is not supported.')
        self.g.rm(path)

    def close(self):
        if self.g is None:
            return
        g, self.g = self.g, None
        try:
            if self.mounted:
                g.umount_all()
            g.shutdown()
        finally:
            g.close()
