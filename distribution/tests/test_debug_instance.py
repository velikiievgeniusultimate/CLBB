import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('debug_instance',ROOT/'tools/debug-instance.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class DebugInstanceTests(unittest.TestCase):
 def test_fresh_profile_does_not_copy_authentication_or_change_primary(self):
  with tempfile.TemporaryDirectory()as td:
   t=Path(td);source=t/'primary';home=t/'home';source.mkdir();settings=home/'.config/clbb/settings.json';settings.parent.mkdir(parents=True);settings.write_text(json.dumps({'values':{'display.resolution':'4000x3111','common.bindings.skill:0.key':'81'}}))
   secret=source/'data/base.apk_/shared_prefs/account.xml';secret.parent.mkdir(parents=True);secret.write_text('must-stay-primary')
   login=source/'data/base.apk_/files/dragon2017/login_data/prefs/session';login.parent.mkdir(parents=True);login.write_text('must-not-be-imported')
   selected={'directory':str(t/'candidate'),'version':'fixture','runtime_sha256':'fixture'}
   with patch.object(module,'ROOT',source),patch.object(module,'BASE',t/'profiles'),patch.object(module,'selection',return_value=selected),patch.object(module.Path,'home',return_value=home),patch.object(module.subprocess,'run'):
    state=module.prepare('secondary');self.assertFalse((state/'data/base.apk_/shared_prefs').exists());self.assertFalse((state/'data/base.apk_/files/dragon2017/login_data').exists());self.assertFalse((state/'data/base.apk_/databases').exists());self.assertEqual(secret.read_text(),'must-stay-primary');self.assertEqual(login.read_text(),'must-not-be-imported');self.assertEqual(module.prepare('secondary'),state)
    values=json.loads((state/'config/clbb/settings.json').read_text())['values'];self.assertEqual(values['display.resolution'],'1280x720');self.assertEqual(values['common.bindings.skill:0.key'],'81');self.assertTrue(json.loads(settings.read_text())['values']['display.resolution']=='4000x3111')
 def test_sandbox_hides_primary_and_isolates_tmp_and_settings(self):
  with tempfile.TemporaryDirectory()as td:
   t=Path(td);state=t/'secondary';state.mkdir();runtime=t/'candidate/runtime';runtime.mkdir(parents=True);(runtime/'VERSION').write_text('fixture');(state/'.clbb-local-runtime.json').write_text(json.dumps({'directory':str(t/'candidate')}))
   with patch.object(module,'ROOT',t/'primary'):
    args=module.sandbox(state,['fixture-game']);self.assertIn(str(t/'primary'),args);self.assertIn('--tmpfs',args);self.assertIn('/tmp',args);self.assertIn(str(state/'config'),args);self.assertIn(str(state/'run'),args);self.assertIn('CLBB_BACKGROUND_STARTUP',args);self.assertIn('CLBB_WRAPPED',args);self.assertEqual(args[-1],'fixture-game')
    self.assertFalse((runtime/'code_cache').exists())
    self.assertIn(['--ro-bind',str(runtime/'VERSION'),str(runtime/'VERSION')],[args[i:i+3] for i in range(len(args))])
    self.assertIn('--remount-ro',args)
    (runtime/'code_cache').mkdir()
    self.assertNotIn('--remount-ro',module.sandbox(state,['fixture-game']))
 def test_names_cannot_escape_private_profile_base(self):
  for name in ['../primary','/tmp/primary','secondary/other','', 'x'*33]:
   with self.assertRaises(ValueError):module.profile(name)
if __name__=='__main__':unittest.main()
