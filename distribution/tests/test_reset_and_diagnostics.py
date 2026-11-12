# SPDX-License-Identifier: GPL-3.0-or-later
import fcntl,json,os
from pathlib import Path
import subprocess,sys,tempfile,unittest
ROOT=Path(__file__).resolve().parents[2]
class ResetAndDiagnostics(unittest.TestCase):
 def fixture(self,p):
  home=p/'home';home.mkdir();root=home/'.local/share/clbb';(root/'releases/v-test').mkdir(parents=True);(root/'state/logs').mkdir(parents=True);(root/'state/game').mkdir();(root/'state/game/base.apk').write_bytes(b'fixture APK');(root/'current').symlink_to('releases/v-test');(home/'unrelated.txt').write_text('keep')
  env=os.environ.copy();env.update(HOME=str(home),XDG_DATA_HOME=str(home/'.local/share'),XDG_CONFIG_HOME=str(home/'.config'),XDG_STATE_HOME=str(home/'.local/state'),CLBB_INSTALL_ROOT=str(root));env.pop('CLBB_STATE_ROOT',None);return home,root,env
 def test_reset_removes_only_application_data(self):
  with tempfile.TemporaryDirectory()as temp:
   home,root,env=self.fixture(Path(temp));(home/'.config/clbb').mkdir(parents=True);(home/'.config/clbb/settings.json').write_text('{}')
   result=subprocess.run(['sh',str(ROOT/'uninstall.sh')],env=env,capture_output=True,text=True)
   self.assertEqual(result.returncode,0,result.stderr);self.assertFalse(root.exists());self.assertFalse((home/'.config/clbb').exists());self.assertEqual((home/'unrelated.txt').read_text(),'keep')
 def test_reset_refuses_running_application(self):
  with tempfile.TemporaryDirectory()as temp:
   home,root,env=self.fixture(Path(temp))
   with(root/'state/logs/clbb-launch.lock').open('a+b')as lock:
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    result=subprocess.run(['sh',str(ROOT/'uninstall.sh')],env=env,capture_output=True,text=True)
   self.assertNotEqual(result.returncode,0);self.assertTrue((root/'state/game/base.apk').exists())
 def test_fallback_records_loader_error_without_form_secrets(self):
  with tempfile.TemporaryDirectory()as temp:
   home,root,env=self.fixture(Path(temp));env['CLBB_DIAGNOSTIC_INVOCATION']='fixture-loader-1';(root/'state/logs/launcher-stderr-fixture-loader-1.log').write_text('error while loading shared libraries: libfoo.so\nCookie: private-cookie\npassword=private-password\n')
   env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
   result=subprocess.run([sys.executable,str(ROOT/'distribution/launch-fallback.py'),str(root/'current'),str(root/'state'),'fixture launch error'],env=env,capture_output=True,text=True,timeout=10)
   self.assertEqual(result.returncode,0,result.stderr);report=(root/'state/logs/last-diagnostic.json').read_text();value=json.loads(report)
   self.assertEqual(value['schema'],'clbb-diagnostic-v1');self.assertIn('libfoo',report);self.assertNotIn('private-cookie',report);self.assertNotIn('private-password',report)
   self.assertEqual(value['launch_provenance']['status'],'unknown');self.assertIsNone(value['last_launch']);self.assertEqual(value['launcher_stderr_provenance']['status'],'explicit_invocation')
if __name__=='__main__':unittest.main()
