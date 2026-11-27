"""Bounded ANR evidence, deliberately not an ANR-fix certification."""
import re


def anr_evidence(serial):
    def section(begin,end):
        if begin not in serial:return ''
        return serial.split(begin,1)[1].split(end,1)[0] if end in serial.split(begin,1)[1] else ''
    baseline=section('NEXUS_ANR_BASELINE_BEGIN','NEXUS_ANR_BASELINE_END')
    after=section('NEXUS_ANR_AFTER_HOME_BEGIN','NEXUS_ANR_AFTER_HOME_END')
    events=section('NEXUS_ANR_EVENTS_BEGIN','NEXUS_ANR_EVENTS_END')
    inventory=section('NEXUS_PACKAGE_INVENTORY_BEGIN','NEXUS_PACKAGE_INVENTORY_END')
    # ANR lines indicate a process was not responding. Exclude "No ANR"
    # negation lines which also contain "ANR in".
    anr_lines=[line for line in (baseline+after).splitlines()
               if re.search(r'ANR in|Not responding|waiting to lock|blocked|timeout',line,re.I)
               and 'No ANR' not in line]
    # Packages present in the guest image (for slimming analysis).
    packages=[line.strip() for line in inventory.splitlines()
              if line.startswith('package:')]
    return {'baseline_captured':bool(baseline.strip()),
            'after_home_captured':bool(after.strip()),
            'events_captured':bool(events.strip()),
            'anr_lines':anr_lines[:20],
            'package_count':len(packages),
            'packages':packages[:50],
            'anr_detected':bool(anr_lines),
            'anr_fixed':False}
