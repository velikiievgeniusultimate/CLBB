# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import io,json,os
from pathlib import Path
import struct,tarfile,tempfile,unittest,zipfile
ROOT=Path(__file__).resolve().parents[1]
def module(name):
    spec=importlib.util.spec_from_file_location(name,ROOT/(name+'.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
extract=module('extract');importer=module('import_game');verify=module('verify')
def manifest(package='com.mobile.legends',split=None,version=42):
    strings=['manifest','package',package,'versionCode']+(['split',split]if split else [])
    data=b'';offsets=[]
    for text in strings:
        b=text.encode();offsets.append(len(data));data+=bytes([len(text),len(b)])+b+b'\0'
    pool=struct.pack('<HHI5I',1,28,28+len(strings)*4+len(data),len(strings),0,256,28+len(strings)*4,0)+struct.pack('<'+'I'*len(strings),*offsets)+data
    def attr(name,raw,kind,value):return struct.pack('<IIIHBBI',0xffffffff,name,raw,8,0,kind,value)
    attrs=attr(1,2,3,2)+attr(3,0xffffffff,0x10,version)
    if split:attrs+=attr(4,5,3,5)
    tag=struct.pack('<HHIII',0x102,16,36+len(attrs),1,0xffffffff)+struct.pack('<II6H',0xffffffff,0,20,20,len(attrs)//20,0,0,0)+attrs
    return struct.pack('<HHI',3,8,8+len(pool)+len(tag))+pool+tag
class DistributionTests(unittest.TestCase):
    def test_archive_rejects_state_and_path_traversal_before_extracting(self):
        for name in ['../outside','data/account.db','/tmp/escape']:
            with self.subTest(name=name),tempfile.TemporaryDirectory()as temp:
                p=Path(temp);archive=p/'x.tar.gz';dest=p/'out';dest.mkdir()
                with tarfile.open(archive,'w:gz')as tar:
                    info=tarfile.TarInfo(name);info.size=1;tar.addfile(info,io.BytesIO(b'x'))
                with self.assertRaises(ValueError):extract.extract(archive,dest)
                self.assertFalse(list(dest.iterdir()))
    def test_archive_accepts_build_revision(self):
        with tempfile.TemporaryDirectory()as temp:
            p=Path(temp);archive=p/'x.tar.gz';dest=p/'out';dest.mkdir()
            with tarfile.open(archive,'w:gz')as tar:
                info=tarfile.TarInfo('REVISION');info.size=40;tar.addfile(info,io.BytesIO(b'a'*40))
            extract.extract(archive,dest)
            self.assertEqual((dest/'REVISION').read_text(),'a'*40)
    def test_archive_rejects_escaping_symlink(self):
        with tempfile.TemporaryDirectory()as temp:
            p=Path(temp);archive=p/'x.tar.gz';dest=p/'out';dest.mkdir()
            with tarfile.open(archive,'w:gz')as tar:
                info=tarfile.TarInfo('prefix/escape');info.type=tarfile.SYMTYPE;info.linkname='../../outside';tar.addfile(info)
            with self.assertRaises(tarfile.FilterError):extract.extract(archive,dest)
    def test_manifest_parser_and_import_preserve_account_data(self):
        with tempfile.TemporaryDirectory()as temp:
            p=Path(temp);state=p/'state';(state/'data').mkdir(parents=True);(state/'data/account.db').write_bytes(b'private fixture')
            for filename,split in [('game.apk',None),('language.apk','config.ru')]:
                with zipfile.ZipFile(p/filename,'w')as z:z.writestr('AndroidManifest.xml',manifest(split=split))
            result=importer.import_game(state,[str(p/'game.apk'),str(p/'language.apk')])
            self.assertEqual(result['apk_count'],2);self.assertEqual((state/'data/account.db').read_bytes(),b'private fixture')
            self.assertEqual((state/'game/base.apk').read_bytes(),(p/'game.apk').read_bytes())
            self.assertFalse(result['account_data_imported'])
    def test_wrong_package_never_replaces_existing_game(self):
        with tempfile.TemporaryDirectory()as temp:
            p=Path(temp);state=p/'state';(state/'game').mkdir(parents=True);(state/'game/base.apk').write_bytes(b'previous')
            with zipfile.ZipFile(p/'wrong.apk','w')as z:z.writestr('AndroidManifest.xml',manifest('wrong.package'))
            with self.assertRaises(ValueError):importer.import_game(state,[str(p/'wrong.apk')])
            self.assertEqual((state/'game/base.apk').read_bytes(),b'previous')
    def test_payload_verification_detects_mutated_file(self):
        import hashlib
        with tempfile.TemporaryDirectory()as temp:
            p=Path(temp);(p/'VERSION').write_text('v0.1.0');(p/'program').write_bytes(b'original')
            data={'version':'v0.1.0','files':{'program':{'type':'file','sha256':hashlib.sha256(b'original').hexdigest()}}};(p/'release.json').write_text(json.dumps(data))
            verify.verify(p,p/'release.json');(p/'program').write_bytes(b'changed')
            with self.assertRaises(ValueError):verify.verify(p,p/'release.json')
if __name__=='__main__':unittest.main()
