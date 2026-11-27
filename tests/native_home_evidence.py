"""Bounded activity/crash evidence, deliberately not a usable/rendered Home gate."""
import re


def home_evidence(serial):
    selected=re.findall(r'^NEXUS_HOME_SELECTED ([A-Za-z0-9_.]+/[A-Za-z0-9_.$]+)\s*$',serial,re.M)
    selected=[value for value in selected if value.split('/',1)[0] not in {'android','com.android.provision','com.android.settings'}]
    component=selected[-1] if selected else None
    def section(begin,end):
        if begin not in serial:return ''
        return serial.split(begin,1)[1].split(end,1)[0] if end in serial.split(begin,1)[1] else ''
    activity=section('NEXUS_HOME_ACTIVITY_STATE_BEGIN','NEXUS_HOME_ACTIVITY_STATE_END')
    crash=section('NEXUS_HOME_CRASH_LOG_BEGIN','NEXUS_HOME_CRASH_LOG_END')
    resumed=bool(component and any(component in line and 'ResumedActivity:' in line for line in activity.splitlines()))
    return {'selected_component':component,'home_activity_resumed':resumed,
            'crash_capture_complete':'NEXUS_HOME_CRASH_LOG_BEGIN' in serial and 'NEXUS_HOME_CRASH_LOG_END' in serial,
            'frame_duration_abort_observed':'Impossible totalDuration' in serial,
            'captured_native_crash':bool(re.search(r'Fatal signal|Abort message:|>>> .* <<<',crash)),
            'keyevent_commands_completed':all(marker in serial for marker in ['NEXUS_HOME_RECENTS_KEY_SENT','NEXUS_HOME_HOME_KEY_SENT']),
            'launcher_process_retained':'NEXUS_HOME_PROCESS_RETAINED' in serial,
            'android_home_verified':False,'rendered_home_verified':False,'input_responsiveness_verified':False}
