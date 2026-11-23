#!/usr/bin/env python3
"""Developer-only file browser for an offline QCOW2 copy (Linux / WSLg)."""
import os
import sys
import tkinter as tk
from tkinter import filedialog, messagebox, simpledialog, ttk
from image_workspace import Workspace, create_copy

class Editor(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title('Nexus Image Studio · Offline files')
        self.geometry('820x570')
        self.workspace = None
        self.devices = {}
        self.protocol('WM_DELETE_WINDOW', self.close)
        ttk.Label(self, text='Offline image workspace', font=('Sans', 18, 'bold')).pack(anchor='w', padx=18, pady=(16, 2))
        ttk.Label(self, text='Edits affect a NEW copy only. Shut down every VM using the source before copying.', wraplength=760).pack(anchor='w', padx=18)
        top = ttk.Frame(self); top.pack(fill='x', padx=18, pady=12)
        ttk.Button(top, text='Create & open working copy', command=self.open_image).pack(side='left')
        self.device = ttk.Combobox(top, state='readonly', width=35); self.device.pack(side='left', padx=8)
        ttk.Button(top, text='Mount filesystem', command=self.mount).pack(side='left')
        self.path = tk.StringVar(value='No image open')
        ttk.Label(self, textvariable=self.path).pack(anchor='w', padx=18)
        self.files = tk.Listbox(self, font=('Monospace', 11)); self.files.pack(fill='both', expand=True, padx=18, pady=8)
        self.files.bind('<Double-Button-1>', lambda e: self.enter())
        controls = ttk.Frame(self); controls.pack(fill='x', padx=18, pady=(0, 12))
        for label, fn in [('Up', self.up), ('Open folder', self.enter), ('Export file', self.export), ('Import / replace', self.upload), ('New folder', self.mkdir), ('Delete file', self.delete)]:
            ttk.Button(controls, text=label, command=fn).pack(side='left', padx=(0, 4))
        ttk.Label(self, text='Encrypted, nested system images and verified-boot metadata are not edited automatically.', wraplength=760).pack(anchor='w', padx=18, pady=(0, 12))
        self.names = []

    def perform(self, action):
        try:
            self.config(cursor='watch'); self.update_idletasks()
            action()
        except Exception as exc:
            messagebox.showerror('Image Studio', str(exc), parent=self)
        finally:
            self.config(cursor='')

    def selected(self):
        sel = self.files.curselection()
        if not sel: raise ValueError('Select a file or folder first.')
        return self.names[sel[0]]

    def open_image(self):
        if self.workspace:
            messagebox.showinfo('Image Studio', 'Close this window before opening another image.', parent=self); return
        if not messagebox.askokcancel('Offline copy', 'Shut down Android and ALL VMs using the source. The source will be copied to a new file and never edited directly. Copying an active disk may produce a corrupt copy. Continue?', parent=self): return
        source = filedialog.askopenfilename(title='Select a stopped standalone QCOW2 image', filetypes=[('QCOW2 images', '*.qcow2'), ('All files', '*')])
        if not source: return
        target = filedialog.asksaveasfilename(title='Choose NEW working copy (will not overwrite)', defaultextension='.qcow2')
        if not target: return
        def run():
            create_copy(source, target)
            self.workspace = Workspace(target)
            self.devices = self.workspace.filesystems()
            self.device['values'] = [f'{dev}  ({fs})' for dev, fs in self.devices.items()]
            if self.device['values']: self.device.current(0)
            self.path.set(f'Working copy: {target} · choose a filesystem')
        self.perform(run)

    def mount(self):
        def run():
            if not self.workspace or self.device.current() < 0: raise ValueError('Open a copy and choose a filesystem first.')
            self.workspace.mount(list(self.devices)[self.device.current()]); self.refresh()
        self.perform(run)

    def refresh(self):
        entries = self.workspace.entries(); self.names = [x[0] for x in entries]
        self.files.delete(0, 'end')
        for name, isdir in entries: self.files.insert('end', ('[DIR]  ' if isdir else '[FILE] ') + name)
        self.path.set('Guest: ' + self.workspace.current)

    def enter(self):
        def run(): self.workspace.enter(self.selected()); self.refresh()
        self.perform(run)
    def up(self):
        def run(): self.workspace.up(); self.refresh()
        self.perform(run)
    def export(self):
        def run():
            name = self.selected(); dest = filedialog.asksaveasfilename(title='Export to NEW host file', initialfile=name)
            if dest: self.workspace.export_file(name, dest)
        self.perform(run)
    def upload(self):
        def run():
            source = filedialog.askopenfilename(title='Choose a host file to import')
            if not source: return
            name = simpledialog.askstring('Import', 'Guest filename:', initialvalue=os.path.basename(source), parent=self)
            if not name: return
            if self.workspace.exists(name) and not messagebox.askyesno('Replace file?', 'Replace this guest file in the working copy? This can make Android unbootable.', parent=self): return
            self.workspace.upload_file(source, name); self.refresh()
        self.perform(run)
    def mkdir(self):
        def run():
            name = simpledialog.askstring('New folder', 'Folder name:', parent=self)
            if name: self.workspace.mkdir(name); self.refresh()
        self.perform(run)
    def delete(self):
        def run():
            name = self.selected()
            if messagebox.askyesno('Delete file?', 'Delete this file from the working copy? This cannot be undone.', parent=self): self.workspace.delete_file(name); self.refresh()
        self.perform(run)
    def close(self):
        if self.workspace:
            try: self.workspace.close()
            except Exception as exc: messagebox.showerror('Could not flush image', str(exc), parent=self); return
        self.destroy()

if __name__ == '__main__':
    Editor().mainloop()
