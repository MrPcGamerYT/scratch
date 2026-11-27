import unittest
from native_home_evidence import home_evidence

class HomeEvidenceTests(unittest.TestCase):
    def sample(self,activity='com.android.launcher3/com.bliss.launcher.BlissLauncher',crash=''):
        return ('NEXUS_HOME_SELECTED com.android.launcher3/com.bliss.launcher.BlissLauncher\n'
                'NEXUS_HOME_ACTIVITY_STATE_BEGIN\nmResumedActivity: '+activity+'\nNEXUS_HOME_ACTIVITY_STATE_END\n'
                'NEXUS_HOME_CRASH_LOG_BEGIN\n'+crash+'\nNEXUS_HOME_CRASH_LOG_END\n')
    def test_resumed_is_not_usable_home(self):
        d=home_evidence(self.sample());self.assertTrue(d['home_activity_resumed']);self.assertFalse(d['android_home_verified']);self.assertFalse(d['rendered_home_verified'])
    def test_resolver_is_not_home(self):
        self.assertFalse(home_evidence(self.sample('android/com.android.internal.app.ResolverActivity'))['home_activity_resumed'])
    def test_missing_evidence_is_not_success(self):
        for text in ('',self.sample().replace('NEXUS_HOME_ACTIVITY_STATE_END','missing')):
            self.assertFalse(home_evidence(text)['home_activity_resumed'])
        self.assertFalse(home_evidence(self.sample().replace('NEXUS_HOME_CRASH_LOG_END','missing'))['crash_capture_complete'])
    def test_provision_and_fallback_never_count_as_home(self):
        for component in ('com.android.provision/.DefaultActivity','com.android.settings/.FallbackHome'):
            text=self.sample(component).replace('NEXUS_HOME_SELECTED com.android.launcher3/com.bliss.launcher.BlissLauncher','NEXUS_HOME_SELECTED '+component)
            self.assertFalse(home_evidence(text)['home_activity_resumed'])
    def test_selection_ignores_package_manager_order(self):
        import subprocess
        from pathlib import Path
        script=Path('guest-tools/nexus-guest/home-diagnostic.sh').read_text()
        selection=script[script.index('eligible='):script.index('if [ -n "$component" ]')]
        for candidates in ('com.android.provision/.DefaultActivity\ncom.android.launcher3/com.bliss.launcher.BlissLauncher',
                           'com.android.settings/.FallbackHome\ncom.farmerbb.taskbar.support/.BlissHomeActivity\ncom.android.launcher3/com.bliss.launcher.BlissLauncher'):
            p=subprocess.run(['sh','-c',selection+'\nprintf "%s" "$component"'],env={'candidates':candidates},capture_output=True,text=True,check=True)
            self.assertEqual(p.stdout,'com.android.launcher3/com.bliss.launcher.BlissLauncher')

    def test_process_ready_and_keyevent_failure_are_distinct(self):
        text=self.sample()
        text=text.replace('NEXUS_HOME_PROCESS_RETAINED','NEXUS_HOME_PROCESS_READY 1\nNEXUS_HOME_RECENTS_KEY_FAILED\nNEXUS_HOME_HOME_KEY_FAILED\n')
        d=home_evidence(text)
        self.assertTrue(d['launcher_process_ready'])
        self.assertFalse(d['keyevent_commands_completed'])
        self.assertFalse(d['launcher_process_retained'])
        # keyevent failure must not look like a clean pass
        self.assertFalse(d['android_home_verified'])

    def test_duration_abort_and_other_native_crashes(self):
        d=home_evidence(self.sample(crash="Abort message: 'Impossible totalDuration 0'"));self.assertTrue(d['frame_duration_abort_observed']);self.assertTrue(d['captured_native_crash'])
        self.assertTrue(home_evidence(self.sample(crash='Fatal signal 11'))['captured_native_crash'])

if __name__=='__main__':unittest.main()
