"""Local tests for bounded ANR evidence parsing."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent))
from native_anr_evidence import anr_evidence

def test_no_anr_sections_yields_empty():
    e=anr_evidence('no markers here')
    assert e['baseline_captured'] is False
    assert e['after_home_captured'] is False
    assert e['anr_detected'] is False
    assert e['anr_fixed'] is False
    assert e['package_count']==0

def test_clean_anr_baseline_no_anr_detected():
    serial='''NEXUS_ANR_BASELINE_BEGIN
ActivityManager: No ANR in last 24 hours
NEXUS_ANR_BASELINE_END
NEXUS_ANR_AFTER_HOME_BEGIN
ActivityManager: No ANR
NEXUS_ANR_AFTER_HOME_END
'''
    e=anr_evidence(serial)
    assert e['baseline_captured'] is True
    assert e['after_home_captured'] is True
    assert e['anr_detected'] is False

def test_anr_lines_detected():
    serial='''NEXUS_ANR_BASELINE_BEGIN
ANR in com.android.systemui
Not responding: com.android.launcher3
NEXUS_ANR_BASELINE_END
'''
    e=anr_evidence(serial)
    assert e['anr_detected'] is True
    assert len(e['anr_lines'])>=2

def test_packages_parsed_from_inventory():
    serial='''NEXUS_PACKAGE_INVENTORY_BEGIN
package:com.android.launcher3
package:com.android.systemui
package:org.nexus.smoke
NEXUS_PACKAGE_INVENTORY_END
'''
    e=anr_evidence(serial)
    assert e['package_count']==3
    assert 'package:com.android.launcher3' in e['packages']

if __name__=='__main__':
    for name,fn in sorted(globals().items()):
        if name.startswith('test_') and callable(fn):
            fn();print(f'{name} PASS')
    print('All ANR evidence tests passed')
