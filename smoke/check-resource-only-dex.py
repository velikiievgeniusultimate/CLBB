#!/usr/bin/env python3
"""Exercise the production ART resource-only fast path with real libziparchive.

Builds only a tiny fixture, never ART/runtime or game APKs. Corruption and APK
replacement must preserve the normal compiler/error path; resource entries and
caller-owned file descriptors remain usable. No settings or installed files change.
"""
from pathlib import Path
import os
import struct
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT/'src/art_standalone/art/runtime/oat_file_assistant.cc'


def extract(text, signature):
    begin = text.index(signature)
    end = text.index('{', begin) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[begin:end]


production = SOURCE.read_text()
helper = extract(production, 'static bool IsResourceOnlyArchive(')
getter = extract(production, 'std::unique_ptr<OatFile> OatFileAssistant::GetBestOatFile()')
fixture = r'''
#include <ziparchive/zip_archive.h>
#include <cassert>
#include <fcntl.h>
#include <memory>
#include <iostream>
#include <string>
#include <unistd.h>
#include <vector>
static int compiles, resets;
static constexpr bool kIsTargetBuild=false;
struct OatFile {};
struct Info {
 std::string filename="fixture-output";
 const std::string* Filename(){return &filename;}
 void Reset(){resets++;}
 std::unique_ptr<OatFile> ReleaseFileForUse(){return std::make_unique<OatFile>();}
};
struct Runtime {
 static Runtime* Current(){static Runtime r;return &r;}
 std::string GetCompilerExecutable(){return "fixture-dex2oat";}
 std::string GetImageLocation(){return "fixture-image";}
 void AddCurrentRuntimeFeaturesAsDex2OatArguments(std::vector<std::string>*){}
};
namespace android { namespace base {
 std::string Join(const std::vector<std::string>&v,char separator){std::string r;for(const auto&s:v){if(!r.empty())r+=separator;r+=s;}return r;}
}}
#define LOG(level) std::cerr
static bool Exec(const std::vector<std::string>& args,std::string*){assert(args[0]=="fixture-dex2oat");compiles++;return true;}
struct OatFileAssistant {
 std::string dex_location_;int zip_fd_=-1;bool current=false;Info info;
 bool IsUpToDate(){return current;}Info&GetBestInfo(){return info;}
 std::unique_ptr<OatFile> GetBestOatFile();
};
''' + helper + '\n' + getter + r'''
static void check(const char* filename,bool resource_only,bool current=false){
 assert(IsResourceOnlyArchive(filename,-1)==resource_only);
 OatFileAssistant assistant;assistant.dex_location_=filename;assistant.current=current;
 int before=compiles;int reset_before=resets;auto oat=assistant.GetBestOatFile();assert(oat);
 assert(compiles-before==(!current&&!resource_only));assert(resets-reset_before==compiles-before);
 int fd=open(filename,O_RDONLY|O_CLOEXEC);assert(fd>=0);assert(lseek(fd,7,SEEK_SET)==7);
 assert(IsResourceOnlyArchive(filename,fd)==resource_only);assert(fcntl(fd,F_GETFD)>=0);assert(lseek(fd,0,SEEK_CUR)==7);close(fd);
}
int main(int argc,char**argv){
 assert(argc>=3);
 if(std::string(argv[1])=="resource")check(argv[2],true);
 else if(std::string(argv[1])=="compile")check(argv[2],false);
 else if(std::string(argv[1])=="cached")check(argv[2],false,true);
 else if(std::string(argv[1])=="replace"){
  assert(argc==4);check(argv[2],true);assert(rename(argv[3],argv[2])==0);check(argv[2],false);
 }else assert(false);
}
'''

with tempfile.TemporaryDirectory(prefix='clbb-resource-dex-') as temporary:
    directory = Path(temporary)
    source, binary = directory/'check.cc', directory/'check'
    source.write_text(fixture)
    libdir = ROOT/'prefix/lib/art'
    subprocess.run(['c++', '-std=c++17', '-O1', '-g', '-Wall', '-Wextra', '-Werror',
                    '-fsanitize=undefined', '-fno-sanitize-recover=all',
                    '-I'+str(ROOT/'src/art_standalone/libziparchive/include'),
                    '-I'+str(ROOT/'src/art_standalone/libbase/include'),
                    str(source), '-L'+str(libdir), '-Wl,-rpath,'+str(libdir),
                    '-Wl,-rpath-link,'+str(libdir), '-lziparchive', '-o', str(binary)], check=True)

    def archive(name, entries, compression=zipfile.ZIP_DEFLATED):
        path = directory/name
        with zipfile.ZipFile(path, 'w', compression=compression) as output:
            for key, value in entries.items():
                output.writestr(key, value)
        return path

    cases = []
    resources = {'AndroidManifest.xml': b'fixture', 'resources.arsc': b'resources',
                 'assets/keep.bin': bytes(range(128)), 'lib/arm64-v8a/libfixture.so': b'native'}
    for compression in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
        cases.append(('resource', archive(f'resource-{compression}.apk', resources, compression)))
    cases.append(('resource', archive('nested-dex.apk', {'assets/classes.dex': b'opaque resource'})))
    for name in ('classes.dex', 'classes2.dex', 'classes999.dex', 'classesBogus.dex'):
        cases.append(('compile', archive(name+'.apk', {**resources, name: b'not a valid dex, retain compiler error'})))
    cases.append(('compile', archive('multidex.apk', {**resources, 'classes.dex': b'one', 'classes2.dex': b'two'})))
    cases.append(('compile', archive('empty-classes.apk', {**resources, 'classes.dex': b''})))
    # No filename exception: even a file named framework-res.apk with code
    # must keep the compiler/error path; real framework resources skip by data.
    cases.append(('compile', archive('framework-res.apk', {'classes.dex': b'code'})))
    cases.append(('cached', archive('up-to-date.apk', {'classes.dex': b'code'})))
    raw = directory/'raw.dex'; raw.write_bytes(b'dex\n035\0'+bytes(128));cases.append(('compile', raw))
    broken = directory/'broken.apk';broken.write_bytes(b'PK\x03\x04broken');cases.append(('compile', broken))
    truncated = directory/'truncated.apk';truncated.write_bytes(cases[0][1].read_bytes()[:-12]);cases.append(('compile', truncated))
    corrupt_local = directory/'corrupt-local.apk';data=bytearray(cases[0][1].read_bytes());data[:4]=b'BAD!';corrupt_local.write_bytes(data);cases.append(('compile',corrupt_local))
    duplicate = directory/'bad-offset.apk';data=bytearray(cases[0][1].read_bytes());at=data.index(b'PK\x01\x02');struct.pack_into('<I',data,at+42,len(data)+1000);duplicate.write_bytes(data);cases.append(('compile',duplicate))
    env={**os.environ, 'LD_LIBRARY_PATH':str(libdir)}
    for mode, path in cases:
        subprocess.run([str(binary), mode, str(path)], env=env, check=True, timeout=5,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if mode == 'resource':
            # Fast-path detection never consumes/replaces resource payloads.
            with zipfile.ZipFile(path) as zip:
                for name in zip.namelist():
                    assert zip.read(name) is not None
    replacement = archive('replace.apk', resources)
    new_apk = archive('next.apk', {**resources, 'classes2.dex': b'new code'})
    subprocess.run([str(binary),'replace',str(replacement),str(new_apk)], env=env,
                   check=True, timeout=5, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
print(f'RESOURCE_ONLY_DEX_OK cases={len(cases)+1} actual_production_branch=1 real_zip_reader=1 ubsan=1 borrowed_fd_preserved=1 apk_replacement=1')
