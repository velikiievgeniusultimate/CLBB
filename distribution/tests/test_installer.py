# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the actual shell installer with isolated release/network fixtures."""
import hashlib,json,os
from pathlib import Path
import subprocess,sys,tempfile,unittest
ROOT=Path(__file__).resolve().parents[2]
class InstallerTests(unittest.TestCase):
    def run_installer(self, *, existing=False, override=None, manifest_url='https://fixture.invalid/game.apk'):
        with tempfile.TemporaryDirectory(prefix='clbb-installer-test-')as temp:
            p=Path(temp);bin=p/'bin';bin.mkdir();home=p/'home';home.mkdir();install=p/'installation'
            if existing:
                (install/'state/game').mkdir(parents=True);(install/'state/game/base.apk').write_bytes(b'preserved')
            helper=p/'manager'
            helper.write_text('#!'+sys.executable+'\nimport json,os,sys\nfrom pathlib import Path\np=Path(os.environ["CLBB_FIXTURE"])\nwith (p/"calls").open("a")as f:f.write(json.dumps(sys.argv[1:])+"\\n")\nprint("{}")\n')
            digest=hashlib.sha256(helper.read_bytes()).hexdigest()
            (p/'SHA256SUMS').write_text(digest+'  clbb-manager-linux-x86_64\n')
            (p/'releases.json').write_text(json.dumps([{'tag_name':'v0.1.0-alpha.3','draft':False}]))
            (p/'game-apk.json').write_text(json.dumps({'package':'com.mobile.legends','download_url':manifest_url,'sha256':'a'*64}))
            curl=bin/'curl';curl.write_text('#!'+sys.executable+'''\nimport os,sys,shutil
from pathlib import Path
args=sys.argv[1:];p=Path(os.environ['CLBB_FIXTURE']);url=next(a for a in args if a.startswith('https://'))
with (p/'requests').open('a')as f:f.write(url+'\\n')
name='releases.json'if '/api.github.com/'in url else 'game-apk.json'if url.endswith('/game-apk.json')else 'SHA256SUMS'if url.endswith('/SHA256SUMS')else 'manager'
flag='--output'if '--output'in args else '-o';shutil.copyfile(p/name,args[args.index(flag)+1])
''');curl.chmod(0o755)
            for tool,text in [('pacman','exit 0'),('id','echo 1000')]:
                f=bin/tool;f.write_text('#!/bin/sh\n'+text+'\n');f.chmod(0o755)
            env=os.environ.copy()
            for key in ['CLBB_APK_FILE','CLBB_APK_URL','CLBB_APK_SHA256','CLBB_INSTALL_GAME','CLBB_STATE_ROOT']:env.pop(key,None)
            env.update(HOME=str(home),CLBB_INSTALL_ROOT=str(install),CLBB_FIXTURE=str(p),PATH=str(bin)+':'+env['PATH']);env.update(override or {})
            result=subprocess.run(['sh',str(ROOT/'install.sh')],env=env,capture_output=True,text=True,timeout=10)
            calls=[json.loads(line)for line in (p/'calls').read_text().splitlines()]if(p/'calls').exists()else[]
            requests=(p/'requests').read_text().splitlines()if(p/'requests').exists()else[]
            preserved=(install/'state/game/base.apk').read_bytes()if existing else None
            return result,calls,requests,preserved
    def test_clean_install_automatically_requests_apk_import(self):
        result,calls,requests,_=self.run_installer();self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(calls,[['--install'],['--import-game-url','https://fixture.invalid/game.apk','a'*64]])
        self.assertTrue(any(url.endswith('/game-apk.json')for url in requests))
    def test_existing_game_is_preserved_without_a_game_download(self):
        result,calls,requests,preserved=self.run_installer(existing=True);self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(calls,[['--install']]);self.assertEqual(preserved,b'preserved');self.assertFalse(any(url.endswith('/game-apk.json')for url in requests))
    def test_game_download_can_be_disabled(self):
        result,calls,_,_=self.run_installer(override={'CLBB_INSTALL_GAME':'0'});self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(calls,[['--install']])
    def test_missing_local_apk_fails_before_network_or_install(self):
        result,calls,requests,_=self.run_installer(override={'CLBB_APK_FILE':'/clbb/does-not-exist.apk'});self.assertNotEqual(result.returncode,0);self.assertEqual(calls,[]);self.assertEqual(requests,[]);self.assertIn('APK file not found',result.stderr)
    def test_non_https_default_source_is_rejected(self):
        result,calls,_,_=self.run_installer(manifest_url='http://fixture.invalid/game.apk');self.assertNotEqual(result.returncode,0);self.assertEqual(calls,[['--install']]);self.assertIn('Invalid game download URL',result.stdout+result.stderr)
if __name__=='__main__':unittest.main()
