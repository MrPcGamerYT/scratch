"""Inventory-only lean-guest review. Never executes guest commands or modifies images.
Consumes the read-only packages.json capture. A candidate is not a safe-to-remove
claim: dependency review and disposable-overlay regression tests are mandatory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

OPTIONAL_REVIEW = {
    'com.android.egg': 'Android Easter egg; optional entertainment UI',
    'com.android.dreams.basic': 'Basic screensaver; optional visual feature',
    'com.android.dreams.phototable': 'Photo screensaver; optional visual feature',
}
PACKAGE = re.compile(r'[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+\Z')
PROTECTED = {
    'android', 'com.android.systemui', 'com.android.settings', 'com.android.shell',
    'com.android.launcher3', 'com.android.documentsui', 'com.android.packageinstaller',
    'com.android.permissioncontroller', 'com.android.providers.settings',
    'com.android.providers.media', 'com.android.providers.downloads',
    'com.android.inputmethod.latin', 'com.android.webview',
}


def plan(raw, home_package, file_editor_package):
    for name in [home_package, file_editor_package]:
        if not PACKAGE.fullmatch(name):
            raise ValueError('Explicit valid Home and file-editor package IDs are required')
    record = json.loads(raw)
    if record.get('ok') is not True:
        raise ValueError('Installed package capture failed; refusing cleanup guesses')
    inventory = {}
    for line in record.get('text', '').splitlines():
        match = re.fullmatch(r'package:(/[^\r\n]+)=([A-Za-z][A-Za-z0-9_.]*)', line.strip())
        if not match or (match[2]!='android' and not PACKAGE.fullmatch(match[2])):
            raise ValueError('Unrecognized pm list packages -f record')
        if match[2] in inventory:
            raise ValueError('Duplicate package ID in capture')
        inventory[match[2]] = match[1]
    for name in [home_package, file_editor_package]:
        if name not in inventory:
            raise ValueError('Protected Home/file editor missing from inventory: ' + name)
    protected = PROTECTED | {home_package, file_editor_package}
    candidates = [{'package': name, 'apk_path': inventory[name], 'reason': reason,
                   'action': 'review_only', 'dependencies_verified': False,
                   'requires_disposable_overlay_test': True}
                  for name, reason in sorted(OPTIONAL_REVIEW.items()) if name in inventory and name not in protected]
    return {'schema': 1, 'inventory_sha256': hashlib.sha256(raw.encode()).hexdigest(),
            'inventory_packages': len(inventory), 'protected_home': home_package,
            'protected_file_editor': file_editor_package, 'review_candidates': candidates,
            'retained_packages': sorted(set(inventory) - {c['package'] for c in candidates}),
            'image_modified': False, 'size_saved_bytes': None,
            'warning': 'No deletion/disable commands generated. Retain every unreviewed package; this plan does not prove dependencies or predict boot speed.'}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--packages-json', required=True)
    p.add_argument('--home-package', required=True)
    p.add_argument('--file-editor-package', required=True)
    p.add_argument('--output', required=True)
    args = p.parse_args()
    result = plan(Path(args.packages_json).read_text(encoding='utf-8'), args.home_package, args.file_editor_package)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
