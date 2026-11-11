# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib,importlib.util,json,os,shutil,subprocess,tempfile,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('runtime_identity',Path(__file__).resolve().parents[2]/'tools/runtime_identity.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class RuntimeIdentityTests(unittest.TestCase):
 def test_only_selected_exact_binary_with_matching_hash_is_allowed(self):
  with tempfile.TemporaryDirectory()as temp:
   root=Path(temp);directory=root/'candidate';runtime=directory/'runtime';exe=runtime/'prefix/bin/android-translation-layer';exe.parent.mkdir(parents=True)
   shutil.copy2(shutil.which('sleep'),exe)
   digest=hashlib.sha256(exe.read_bytes()).hexdigest()
   (runtime/'release.json').write_text(json.dumps({'files':{'prefix/bin/android-translation-layer':{'sha256':digest}}}))
   (directory/'candidate.json').write_text(json.dumps({'runtime':'runtime.tar.gz','assets':{'runtime.tar.gz':'a'*64}}))
   marker={'directory':str(directory),'runtime_sha256':'a'*64};(root/'.clbb-local-runtime.json').write_text(json.dumps(marker))
   process=subprocess.Popen([str(exe),'10'])
   try:
    self.assertTrue(module.runtime_executable_is_owned(process.pid,root))
    self.assertFalse(module.runtime_executable_is_owned(os.getpid(),root))
    marker['runtime_sha256']='b'*64;(root/'.clbb-local-runtime.json').write_text(json.dumps(marker))
    self.assertFalse(module.runtime_executable_is_owned(process.pid,root))
   finally:process.terminate();process.wait(timeout=5)
   self.assertFalse(module.runtime_executable_is_owned(process.pid,root))
