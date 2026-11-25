import importlib.util
import pathlib
import tempfile
import unittest
import sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))

spec = importlib.util.spec_from_file_location('probe', pathlib.Path(__file__).with_name('android_boot_probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class RecipeTests(unittest.TestCase):
    def test_preserves_guest_options_and_skips_installer(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            (root / 'kernel').touch()
            (root / 'initrd.img').touch()
            (root / 'grub.cfg').write_text('linux /kernel INSTALL=1\ninitrd /initrd.img\nlinux /kernel quiet root=/dev/ram0 SRC= DATA=\ninitrd /initrd.img\n')
            kernel, initrd, args, config = probe.boot_recipe(root)
            self.assertEqual(kernel.name, 'kernel')
            self.assertEqual(initrd.name, 'initrd.img')
            self.assertNotIn('quiet', args)
            self.assertIn('SRC= DATA=', args)
            self.assertNotIn('INSTALL', args)

    def test_bliss_isolinux_live_recipe(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            (root / 'kernel').touch()
            (root / 'initrd.img').touch()
            (root / 'isolinux.cfg').write_text('label live\n kernel /kernel\n append initrd=/initrd.img root=/dev/ram0 quiet SRC= DATA=\nlabel install\n kernel /kernel\n append initrd=/initrd.img INSTALL=1\n')
            self.assertEqual(probe.boot_recipe(root)[2], 'root=/dev/ram0 SRC= DATA=')

    def test_boot_complete_does_not_hide_renderer_crash(self):
        props = '[sys.boot_completed]: [1]\n[init.svc.surfaceflinger]: [running]\n[init.svc.bootanim]: [stopped]'
        crash = 'F libc : Fatal signal 11 (SIGSEGV) in tid 2222 (llvmpipe-0), pid 2170 (oid.setupwizard)'
        self.assertFalse(probe.assess_ui(props, crash)[0])
        self.assertTrue(probe.assess_ui(props, '')[0])
        self.assertFalse(probe.assess_ui(props.replace('[stopped]', '[running]'), '')[0])

    def test_diagnostic_profiles_are_explicit_and_preserve_boot_args(self):
        cpu, args = probe.profile_recipe('drm-gbm-no-avx', 'root=/dev/ram0 SRC= DATA= HWC=old GRALLOC=old')
        self.assertIn('avx=off', cpu)
        self.assertEqual(args, 'root=/dev/ram0 SRC= DATA= HWC=drm GRALLOC=gbm')
        self.assertEqual(probe.profile_recipe('baseline', 'root=/dev/ram0 SRC=')[1], 'root=/dev/ram0 SRC=')
        with self.assertRaises(KeyError):
            probe.profile_recipe('unvalidated', '')

    def test_virgl_must_be_the_reported_gles_renderer(self):
        self.assertTrue(probe.virgl_renderer_line('GLES: Mesa/X.org, virgl, OpenGL ES 3.1'))
        self.assertFalse(probe.virgl_renderer_line('GLES: Mesa/X.org, llvmpipe\nInstalled driver: virgl'))
        self.assertFalse(probe.virgl_renderer_line('Vulkan: virtio'))

    def test_frame_metrics_reject_invalid_and_do_not_claim_fps(self):
        text = 'Flags,IntendedVsync,FrameCompleted\n0,1000000,21000000\n1,1000000,91000000\n0,0,100000000\n0,1000000,9223372036854775807\n'
        metrics = probe.summarize(text)
        self.assertEqual(metrics['frames'], 1)
        self.assertEqual(metrics['median_ms'], 20)
        self.assertEqual(metrics['status'], 'insufficient_samples')
        self.assertNotIn('fps', metrics)
        self.assertEqual(probe.summarize('no frame data')['frames'], 0)

    def test_home_gate_rejects_setup_and_background_launcher(self):
        from android_home_gate import choose_home, foreground_is_home
        home = choose_home('com.android.setupwizard/.Setup\ncom.android.launcher3/.Launcher\n')
        self.assertEqual(home, 'com.android.launcher3/.Launcher')
        self.assertFalse(choose_home('com.android.setupwizard/.Setup'))
        self.assertFalse(foreground_is_home('background com.android.launcher3/.Launcher', home))
        self.assertTrue(foreground_is_home('mResumedActivity: ActivityRecord{ u0 com.android.launcher3/.Launcher t1}', home))
        self.assertFalse(foreground_is_home('mResumedActivity: com.android.setupwizard/.Setup', home))

    def test_unknown_recipe_fails_instead_of_guessing(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(RuntimeError):
                probe.boot_recipe(pathlib.Path(folder))


if __name__ == '__main__':
    unittest.main()
