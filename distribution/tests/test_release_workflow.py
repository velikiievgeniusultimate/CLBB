# SPDX-License-Identifier: GPL-3.0-or-later
import fcntl,importlib.util,json,os,shlex,subprocess,tempfile,unittest
from unittest.mock import patch
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

 def test_active_profile_cannot_be_switched_out_from_under_diagnostics(self):
  with tempfile.TemporaryDirectory()as temp:
   p=Path(temp);(p/'logs').mkdir()
   with (p/'logs/clbb-launch.lock').open('a+b')as lock,patch.object(workflow,'ROOT',p):
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    with self.assertRaisesRegex(ValueError,'Close CLBB'):workflow.use_local(p/'unused')

 def test_switching_the_runtime_preserves_the_shortcuts_account_and_network_policy(self):
  with tempfile.TemporaryDirectory() as directory:
   root = Path(directory)
   wrapper = root / '.clbb-local-launch'
   old, new = root / 'old runtime', root / 'new runtime'
   with patch.object(workflow, 'ROOT', root):
    workflow.write_local_launcher(wrapper, old)
    default = wrapper.read_text()
    self.assertIn('exec "$CLBB_ROOT/bin/clbb-launcher" "$@"', default)
    routed = default.replace('set -eu\n', 'set -eu\nif [ "$#" -eq 0 ]; then exec python3 accounts.py desktop; fi\n')
    routed = routed.replace('exec "$CLBB_ROOT', 'exec systemd-run --scope --slice=clbbdirect.slice "$CLBB_ROOT')
    wrapper.write_text(routed)
    workflow.write_local_launcher(wrapper, new)
   self.assertEqual(wrapper.read_text(), routed.replace(shlex.quote(str(old)), shlex.quote(str(new))))
   self.assertTrue(wrapper.stat().st_mode & 0o100)

 def test_explicit_selection_retires_deferred_activation_and_its_self_exec(self):
  with tempfile.TemporaryDirectory() as directory:
   root = Path(directory)/'working copy'
   new = root/'new runtime'
   launcher = new/'bin/clbb-launcher'
   launcher.parent.mkdir(parents=True)
   launcher.write_text('#!/bin/sh\necho "$1"\n')
   launcher.chmod(0o755)
   wrapper = root/'.clbb-local-launch'
   pending = root/'logs/older build'
   activation = shlex.join(['python3', str(pending/'activate-next-launch.py'), str(pending/'pending-activation.json')])
   with patch.object(workflow, 'ROOT', root):
    workflow.write_local_launcher(wrapper, root/'old')
    custom = 'export CLBB_ACCOUNT_BYPASS=1\n'
    wrapper.write_text(wrapper.read_text().replace('set -eu\n', 'set -eu\n' + custom + activation + '\nexec "$0" "$@"\n'))
    workflow.write_local_launcher(wrapper, new)
    repaired = wrapper.read_text()
    workflow.write_local_launcher(wrapper, new)
   self.assertEqual(wrapper.read_text(), repaired)
   self.assertIn(custom, repaired)
   self.assertNotIn(activation, repaired)
   self.assertEqual(subprocess.check_output([wrapper, 'ready'], text=True, timeout=3).strip(), 'ready')

 def test_native_only_updates_keep_art_paths_but_changed_jars_get_new_paths(self):
  with tempfile.TemporaryDirectory() as directory:
   workbench = Path(directory)
   def candidate(name, jar):
    runtime = workbench/'candidates'/name/'runtime'
    inputs = {'prefix/lib/art/libart.so': b'vm', 'prefix/bin/dex2oat': b'compiler',
              'prefix/lib/java/dex/art/core-oj-hostdex.jar': jar,
              'prefix/bin/android-translation-layer': name.encode()}
    entries = {}
    for path, content in inputs.items():
     dest = runtime/path
     dest.parent.mkdir(parents=True, exist_ok=True)
     dest.write_bytes(content)
     entries[path] = {'type': 'file', 'sha256': workflow.sha(dest)}
    (runtime/'release.json').write_text(json.dumps({'files': entries}))
    (runtime/'env.sh').write_bytes((ROOT.parent/'env.sh').read_bytes())
    return runtime

   def paths(runtime, profile='profile'):
    env = dict(os.environ, CLBB_STATE_ROOT=str(workbench/profile), LD_LIBRARY_PATH='',
               CLBB_ART_CACHE_DIR='/previous-profile')
    values = subprocess.check_output(['bash','-c',
      'source "$1"; printf "%s\\n" "$LD_LIBRARY_PATH" "${CLBB_ART_CACHE_DIR-}"',
      'test',str(runtime/'env.sh')], env=env, text=True).splitlines()
    return Path(values[0].split(':')[1]), values[1]

   first, second = candidate('first', b'bootstrap'), candidate('second', b'bootstrap')
   self.assertEqual(paths(first), (first/'prefix/lib/art', ''))
   workflow.select_art_components(first, workbench)
   workflow.select_art_components(second, workbench)
   shared, cache = paths(first)
   self.assertTrue(shared.is_relative_to(workbench/'art-components'))
   self.assertTrue(Path(cache).is_relative_to(workbench/'profile/cache'))
   self.assertEqual(paths(second), (shared, cache))
   self.assertNotEqual(paths(first, 'secondary')[1], cache)
   updated = candidate('updated', b'new bytecode')
   workflow.select_art_components(updated, workbench)
   new_shared, new_cache = paths(updated)
   self.assertNotEqual(new_shared, shared)
   self.assertNotEqual(new_cache, cache)
   self.assertEqual((shared/'../java/dex/art/core-oj-hostdex.jar').read_bytes(), b'bootstrap')
   (second.parent/'.clbb-art-key').write_text('"not-a-key"\n')
   self.assertEqual(paths(second), (second/'prefix/lib/art', ''))

 def test_runtime_switch_reindexes_kde_only_when_the_application_entry_changes(self):
  with tempfile.TemporaryDirectory() as directory:
   home = Path(directory)
   root = home/'workbench'
   (root/'logs').mkdir(parents=True)
   (home/'Desktop').mkdir()
   icon = home/'Desktop/CLBB.desktop'
   icon.write_text(f'Exec="{root}/.clbb-local-launch"\n')
   with patch.object(workflow, 'ROOT', root), patch.object(Path, 'home', return_value=home), \
        patch.object(workflow.shutil, 'which', return_value='/usr/bin/kbuildsycoca6'), \
        patch.object(workflow.subprocess, 'run') as kde:
    wrapper, data = root/'.clbb-local-launch', home/'share'
    workflow.write_workbench_desktop(wrapper, data)
    entry = data/'applications/org.clbb.CLBB.desktop'
    stats = [(p.stat().st_mtime_ns, p.stat().st_ctime_ns) for p in (entry, icon)]
    self.assertEqual(icon.read_text(), entry.read_text())
    kde.assert_called_once()
    workflow.write_workbench_desktop(wrapper, data)
    self.assertEqual(stats, [(p.stat().st_mtime_ns, p.stat().st_ctime_ns) for p in (entry, icon)])
    kde.assert_called_once()
