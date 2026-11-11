# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util,fcntl,os,subprocess,sys
from pathlib import Path
import tempfile,unittest,zipfile
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('native_libs',ROOT/'native_libs.py');native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
def elf(payload=b'fixture'):
    return b'\x7fELF\x02\x01'+b'\0'*12+b'\xb7\x00'+payload
def apk(path,entries):
    with zipfile.ZipFile(path,'w')as z:
        for name,body in entries.items():z.writestr(name,body)
class NativeLibraries(unittest.TestCase):
 def state(self,p):
    (p/'game').mkdir();(p/'data').mkdir();(p/'data/account').write_text('preserve')
 def test_prepares_arm64_not_host_or_32bit_and_caches(self):
    with tempfile.TemporaryDirectory()as temp:
     p=Path(temp);self.state(p);apk(p/'game/base.apk',{'lib/arm64-v8a/libmain.so':elf(),'lib/armeabi-v7a/libmain.so':b'wrong','lib/x86_64/libmain.so':b'also wrong'})
     self.assertEqual(native.prepare(p)['state'],'prepared');target=p/'data/base.apk_/lib/libmain.so';self.assertEqual(target.read_bytes(),elf());self.assertEqual(native.prepare(p)['state'],'current');target.unlink();self.assertEqual(native.prepare(p)['state'],'prepared');self.assertEqual((p/'data/account').read_text(),'preserve')
 def test_prepares_libraries_from_matching_split_apks(self):
    with tempfile.TemporaryDirectory()as temp:
     p=Path(temp);self.state(p);apk(p/'game/base.apk',{});apk(p/'game/split_config.arm64.apk',{'lib/arm64-v8a/libmain.so':elf(),'lib/arm64-v8a/libEncryptorP.so':elf(b'sdk')});self.assertEqual(native.prepare(p)['libraries'],2)
 def test_32bit_only_apk_fails_before_writing_libraries(self):
    with tempfile.TemporaryDirectory()as temp:
     p=Path(temp);self.state(p);apk(p/'game/base.apk',{'lib/armeabi-v7a/libmain.so':b'wrong'})
     with self.assertRaises(ValueError):native.prepare(p)
     self.assertFalse((p/'data/base.apk_/lib').exists());self.assertEqual((p/'data/account').read_text(),'preserve')
 def test_refuses_preparation_while_game_is_running(self):
    with tempfile.TemporaryDirectory()as temp:
     p=Path(temp);self.state(p);apk(p/'game/base.apk',{'lib/arm64-v8a/libmain.so':elf()});(p/'logs').mkdir()
     with(p/'logs/clbb-launch.lock').open('a+b')as lock:
      fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
      with self.assertRaises(ValueError):native.prepare(p)
 def test_startup_preparation_uses_the_launcher_held_lock(self):
    with tempfile.TemporaryDirectory()as temp:
     p=Path(temp);self.state(p);apk(p/'game/base.apk',{'lib/arm64-v8a/libmain.so':elf()});(p/'logs').mkdir()
     with(p/'logs/clbb-launch.lock').open('a+b')as lock:
      fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
      env=os.environ.copy();env.update(CLBB_STATE_ROOT=str(p),CLBB_NATIVE_LOCK_HELD='1')
      result=subprocess.run([sys.executable,str(ROOT.parent/'prepare-native-plugins.py')],env=env,capture_output=True,text=True,timeout=10)
     self.assertEqual(result.returncode,0,result.stderr);self.assertEqual((p/'data/base.apk_/lib/libmain.so').read_bytes(),elf());self.assertEqual((p/'data/account').read_text(),'preserve')
if __name__=='__main__':unittest.main()
