# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util,json,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('workflow',ROOT/'release.py');workflow=importlib.util.module_from_spec(spec);spec.loader.exec_module(workflow)
class ReleaseWorkflowTests(unittest.TestCase):
 def test_changed_artifact_cannot_be_used_after_acceptance(self):
  with tempfile.TemporaryDirectory()as temp:
   p=Path(temp);(p/'runtime.tar.gz').write_bytes(b'original')
   (p/'candidate.json').write_text(json.dumps({'assets':{'runtime.tar.gz':workflow.sha(p/'runtime.tar.gz')}}))
   workflow.bundle(p)
   (p/'runtime.tar.gz').write_bytes(b'changed')
   with self.assertRaisesRegex(ValueError,'Candidate changed'):workflow.bundle(p)
 def test_external_asset_path_is_rejected(self):
  with tempfile.TemporaryDirectory()as temp:
   p=Path(temp);(p/'candidate.json').write_text(json.dumps({'assets':{'../outside':'ignored'}}))
   with self.assertRaisesRegex(ValueError,'Candidate changed'):workflow.bundle(p)
 def test_extra_runtime_file_rejected_even_if_manifest_files_are_intact(self):
  with tempfile.TemporaryDirectory()as temp:
   p=Path(temp);runtime=p/'runtime';runtime.mkdir();(runtime/'VERSION').write_text('v1.0.0\n')
   digest=workflow.sha(runtime/'VERSION')
   (runtime/'release.json').write_text(json.dumps({'version':'v1.0.0','files':{'VERSION':{'type':'file','sha256':digest}}}))
   (runtime/'unexpected.so').write_bytes(b'not in the candidate archive')
   (p/'payload.tar.gz').write_bytes(b'fixture')
   (p/'candidate.json').write_text(json.dumps({'version':'v1.0.0','runtime':'payload.tar.gz','assets':{'payload.tar.gz':workflow.sha(p/'payload.tar.gz')}}))
   with self.assertRaisesRegex(ValueError,'Unexpected files'):workflow.verify(p)
