# SPDX-License-Identifier: MIT
"""Exact launch provenance fixtures, without game input or real crash collection."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('clbb_diagnostic_fallback', ROOT/'distribution/launch-fallback.py')
fallback = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fallback)


class DiagnosticProvenance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base/'selected-new-source'
        self.root.mkdir()
        (self.root/'VERSION').write_text('v99-source-not-launched')
        (self.root/'REVISION').write_text('unrelated-repository-head')
        self.state = self.base/'state'
        self.logs = self.state/'logs'
        self.logs.mkdir(parents=True)
        self.directory = self.logs/'qemu-launch-215845032545626-4294967294'
        self.directory.mkdir()
        self.record = {
            'schema':'clbb-launch-scope-v2', 'backend':'qemu',
            'pid':4294967293, 'launcher_pid':4294967294,
            'pid_start_ticks':7, 'launcher_start_ticks':6,
            'start_monotonic_ns':215845032545626,
            'start_unix_ns':100_100_000_000, 'boot_id':fallback.boot_id(),
            'runtime_root':str(self.base/'immutable-alpha32'),
            'runtime_version':'v32-actually-launched', 'runtime_revision':'revision32',
            'launch_directory':str(self.directory), 'invocation_id':'wrapper-current',
        }
        self.publish()
        (self.directory/'runtime.log').write_text('fatal CURRENT32 failure\nCookie: secret-current\n')
        self.old = self.logs/'qemu-launch-99567360239857-1534559'
        self.old.mkdir()
        (self.old/'runtime.log').write_text('fatal STALE1534560 OpenGL error\n')
        (self.logs/'launcher-stderr.log').write_text('fatal STALE-SHARED-STDERR\n')

    def publish(self, actual=None):
        (self.logs/'current-atl-pid.json').write_text(json.dumps(self.record))
        (self.directory/'launch.json').write_text(json.dumps(self.record if actual is None else actual))

    def collect(self, marker=None, reason='fixture crash'):
        with mock.patch.object(fallback, 'run', return_value=''):
            return fallback.collect(self.root, self.state, reason, marker)

    def test_canonical_identity_beats_lexicographically_later_stale_run_and_source_head(self):
        value = self.collect()
        self.assertIn('CURRENT32', value['startup_errors'])
        self.assertNotIn('STALE', value['startup_errors'])
        self.assertNotIn('secret-current', json.dumps(value))
        self.assertEqual(value['last_launch'], str(self.directory))
        self.assertEqual(value['app_version'], 'v32-actually-launched')
        self.assertEqual(value['app_revision'], 'revision32')
        self.assertEqual(value['diagnostic_runtime']['revision'], 'unrelated-repository-head')
        self.assertEqual(value['launch_provenance']['status'], 'verified_launch')

    def test_mismatched_launch_pid_birth_manifest_or_start_never_attaches_old_or_current_log(self):
        for key in ['pid','pid_start_ticks','launcher_start_ticks','start_monotonic_ns','start_unix_ns','runtime_revision','invocation_id']:
            with self.subTest(key=key):
                actual = dict(self.record)
                actual[key] = 'different' if isinstance(actual[key], str) else actual[key]+1
                self.publish(actual)
                value = self.collect()
                self.assertEqual(value['launch_provenance']['status'], 'unknown')
                self.assertIsNone(value['last_launch'])
                self.assertEqual(value['startup_errors'], '')
                self.assertEqual(value['app_version'], 'unknown')

    def test_malformed_launch_or_exit_record_is_unknown_instead_of_crashing_diagnostics(self):
        for malformed in [[],None,'not-an-object']:
            with self.subTest(malformed=malformed):
                (self.directory/'launch.json').write_text(json.dumps(malformed))
                value=self.collect()
                self.assertEqual(value['launch_provenance']['status'],'unknown')
                self.assertIsNone(value['last_launch'])
                (self.directory/'exit.json').write_text(json.dumps(malformed))
                self.assertEqual(fallback.launch_exit((self.directory,self.record)),{})

    def test_legacy_missing_pointer_and_old_boot_are_unknown_without_scanning(self):
        for value in [None, {'pid':1534560,'launcher_pid':1534559,'start_monotonic_ns':99567360239857}, dict(self.record,boot_id='old-boot')]:
            with self.subTest(value=value):
                pointer = self.logs/'current-atl-pid.json'
                if value is None:pointer.unlink(missing_ok=True)
                else:pointer.write_text(json.dumps(value))
                report = self.collect()
                self.assertEqual(report['launch_provenance']['status'],'unknown')
                self.assertIsNone(report['last_launch'])
                self.assertNotIn('STALE', report['startup_errors'])

    def test_reused_runtime_pid_is_rejected_even_when_both_saved_records_match(self):
        self.record.update(pid=os.getpid(),pid_start_ticks=fallback.birth(os.getpid())+1)
        self.publish()
        value = self.collect()
        self.assertIsNone(value['last_launch'])
        self.assertIn('reused', value['launch_provenance']['reason'])

    def test_wrapper_invocation_mismatch_is_unknown_but_its_own_loader_stderr_is_retained(self):
        (self.logs/'launcher-stderr-wrapper-new.log').write_text('error while loading shared libraries: libfoo.so\npassword=private\n')
        value = self.collect('wrapper-new')
        self.assertIsNone(value['last_launch'])
        self.assertIn('different wrapper invocation',value['launch_provenance']['reason'])
        self.assertIn('libfoo',value['startup_errors'])
        self.assertNotIn('CURRENT32',value['startup_errors'])
        self.assertNotIn('private',value['startup_errors'])
        self.assertEqual(value['launcher_stderr_provenance']['status'],'explicit_invocation')

    def test_actual_signals_override_unrelated_graphics_or_dependency_messages(self):
        for reason in ['SIGILL','signal: 4 (SIGILL) (core dumped)']:
            result=fallback.summarize(reason,'libfoo.so => not found\nEGL failure',[])
            self.assertIn('SIGILL',result)
            self.assertIn('не доказывает',result)
            self.assertNotIn('Неподдерживаемая',result)
        self.assertIn('SIGKILL',fallback.summarize('signal: 9 (SIGKILL)','EGL error',[]))
        self.assertIn('SIGSEGV',fallback.summarize('SIGSEGV','',[]))

    def test_coredump_stack_requires_matching_exit_and_is_bounded_by_boot_and_launch_interval(self):
        context=(self.directory,self.record)
        with mock.patch.object(fallback,'run',return_value='Stack trace of thread 5:\n #0 safe_symbol\nENV=not-collected') as command:
            self.assertEqual(fallback.crash_stack(context,{}),'')
            command.assert_not_called()
            value=fallback.crash_stack(context,{'finished_unix_ns':102_000_000_001})
            args=command.call_args.args[0]
            self.assertIn('COREDUMP_PID=4294967293',args)
            self.assertIn('_BOOT_ID='+self.record['boot_id'].replace('-',''),args)
            self.assertIn('--since=@100',args)
            self.assertIn('--until=@103',args)
            self.assertIn('safe_symbol',value)
            self.assertNotIn('ENV=',value)
        (self.directory/'exit.json').write_text(json.dumps({'pid':123,'launcher_pid':4294967294,'start_monotonic_ns':self.record['start_monotonic_ns']}))
        self.assertEqual(fallback.launch_exit(context),{})

    def test_generated_wrapper_fallback_has_its_own_stderr_even_if_launcher_elf_does_not_load(self):
        # Exercise the actual manager template, without package install/network.
        install=self.base/'installed'
        runtime=install/'releases/v-test'
        (runtime/'bin').mkdir(parents=True)
        (runtime/'distribution').mkdir()
        (install/'current').symlink_to('releases/v-test')
        shutil.copy(ROOT/'distribution/launch-fallback.py',runtime/'distribution/launch-fallback.py')
        launcher=runtime/'bin/clbb-launcher'
        launcher.write_text('#!/bin/sh\n[ "$#" -eq 0 ] || exit 1\nprintf "%s\\n" "error while loading shared libraries: libwrapper.so" >&2\nexit 127\n')
        launcher.chmod(0o755)
        source=(ROOT/'distribution/manager/src/main.rs').read_text()
        template=source.split('let script=format!(r#"',1)[1].split('"#);',1)[0]
        script=self.base/'clbb'
        script.write_text(template.replace('{quoted}',"'"+str(install)+"'"))
        env=os.environ.copy()
        env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
        result=subprocess.run(['sh',str(script)],env=env,text=True,capture_output=True,timeout=10)
        self.assertEqual(result.returncode,127,result.stderr)
        value=json.loads((install/'state/logs/last-diagnostic.json').read_text())
        self.assertIn('libwrapper.so',value['startup_errors'])
        self.assertEqual(value['launcher_stderr_provenance']['status'],'explicit_invocation')
        self.assertEqual(value['launch_provenance']['status'],'unknown')
        self.assertIsNone(value['last_launch'])


if __name__ == '__main__':unittest.main()
