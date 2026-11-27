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
    frames=section('NEXUS_HOME_FRAME_STATS_BEGIN','NEXUS_HOME_FRAME_STATS_END')
    frame_count=re.search(r'Total frames rendered:\s*(\d+)',frames)
    process=section('NEXUS_HOME_RECENTS_BEGIN','NEXUS_HOME_RECENTS_END')
    pid_before=re.search(r'^NEXUS_HOME_PID_BEFORE\\s+(\\d+)',process,re.M)
    pid_after=re.search(r'^NEXUS_HOME_PID_AFTER\\s+(\\d+)',process,re.M)
    window_visible='NEXUS_HOME_WINDOW_VISIBLE' in serial
    return {'selected_component':component,'home_activity_resumed':resumed,
            'window_visible':window_visible,
            'crash_capture_complete':'NEXUS_HOME_CRASH_LOG_BEGIN' in serial and 'NEXUS_HOME_CRASH_LOG_END' in serial,
            'frame_duration_abort_observed':'Impossible totalDuration' in serial,
            'captured_native_crash':bool(re.search(r'Fatal signal|Abort message:|>>> .* <<<',crash)),
            'keyevent_commands_completed':all(marker in serial for marker in ['NEXUS_HOME_RECENTS_KEY_SENT','NEXUS_HOME_HOME_KEY_SENT']),
            'keyevent_attempts_made':any(marker in serial for marker in ['NEXUS_HOME_RECENTS_KEY_SENT','NEXUS_HOME_RECENTS_KEY_SENT_RETRY','NEXUS_HOME_HOME_KEY_SENT','NEXUS_HOME_HOME_KEY_SENT_RETRY']),
            'launcher_process_ready':'NEXUS_HOME_PROCESS_READY 1' in serial,
            'launcher_pid_before':int(pid_before.group(1)) if pid_before else None,
            'launcher_pid_after':int(pid_after.group(1)) if pid_after else None,
            'launcher_process_retained':'NEXUS_HOME_PROCESS_RETAINED' in serial,
            'launcher_reported_frames':int(frame_count.group(1)) if frame_count else None,
            'android_home_verified':False,'rendered_home_verified':False,'input_responsiveness_verified':False}
