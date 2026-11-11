#!/usr/bin/env python3
"""Cold Makefile contract: real preprocessing, fake shared-library link; no guest execution."""
import json, os, pathlib, shutil, subprocess, tempfile, time

root = pathlib.Path(__file__).resolve().parents[1]
source = root / 'src/nb-qemu/libnb-qemu-guest/Makefile'
real_clang = shutil.which('clang')
assert real_clang
with tempfile.TemporaryDirectory(prefix='clbb-guest-flags-') as temporary:
    top = pathlib.Path(temporary)
    guest = top / 'libnb-qemu-guest'
    guest.mkdir()
    shutil.copyfile(source, guest / 'Makefile')
    (top / 'libnb-qemu').mkdir()
    (top / 'bin').mkdir()
    journal = top / 'links.jsonl'
    compiler = top / 'bin/clang'
    compiler.write_text('#!/usr/bin/env python3\n' + '''import json, os, pathlib, subprocess, sys
args = sys.argv[1:]
if '-print-resource-dir' in args or '-MM' in args:
    raise SystemExit(subprocess.run([os.environ['REAL_CLANG']] + args).returncode)
with open(os.environ['LINK_JOURNAL'], 'a') as f: f.write(json.dumps(args) + '\\n')
pathlib.Path(args[args.index('-o')+1]).write_bytes(b'cold fake ELF; never executable')
''')
    compiler.chmod(0o700)
    for rel in ['libnb.c', 'jnienv.c', 'libEGL.itf.c', 'liblog.c', 'libOpenSLES.iid.c', 'libOpenSLES.itf.c', 'bionic/libc/arch-common/bionic/crtbegin_so.c', 'bionic/libc/arch-common/bionic/crtend_so.S', 'libstdc++_standalone/new.cpp', 'libstdc++_standalone/__cxa_pure_virtual.cpp', 'libstdc++_standalone/__cxa_guard.cpp']:
        path = guest / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        header = 'jni-test.h' if rel == 'jnienv.c' else 'std-test.h' if rel.endswith('/new.cpp') else None
        path.write_text(('#include "' + header + '"\n') if header else '/* cold fixture */\n')
    (guest / 'jni-test.h').write_text('#define JNI_FIXTURE 1\n')
    (guest / 'libstdc++_standalone/std-test.h').write_text('#define STD_FIXTURE 1\n')
    for library in ['libandroid', 'libEGL', 'libGLESv2', 'libOpenSLES']:
        (top / 'libnb-qemu' / (library + '.def')).write_text('cold fixture\n')
    generator = top / 'genlib.py'
    generator.write_text('#!/usr/bin/env python3\nprint("/* generated cold fixture */")\n')
    generator.chmod(0o700)
    build = top / 'out'
    env = dict(os.environ, PATH=str(top / 'bin') + os.pathsep + os.environ['PATH'], REAL_CLANG=real_clang, LINK_JOURNAL=str(journal))
    command = ['make', '--no-print-directory', '-C', str(guest), 'TOPDIR=' + str(top), 'BUILDDIR=' + str(build), 'TARGETARCH_FULL=aarch64', 'SHELL=/bin/bash']
    def run(*flags):
        result = subprocess.run(command + list(flags), env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        assert result.returncode == 0, result.stdout + result.stderr
        return [json.loads(s) for s in journal.read_text().splitlines()] if journal.exists() else []
    links = run()
    assert len(links) == 7
    assert all('-O2' not in args for args in links)
    assert all('.guest-build-flags' not in args for args in links), 'stamp passed to linker'
    assert '-fno-exceptions' in next(args for args in links if args[args.index('-o')+1].endswith('libstdc++.so'))
    assert len(run()) == 7, 'unchanged command rebuilt libraries'
    time.sleep(.02)
    (guest / 'jni-test.h').write_text('#define JNI_FIXTURE 2\n')
    links = run()
    assert len(links) == 8 and links[-1][links[-1].index('-o')+1].endswith('libnb-qemu-guest.so'), 'JNI header did not rebuild only its dependent library'
    time.sleep(.02)
    (guest / 'libstdc++_standalone/std-test.h').write_text('#define STD_FIXTURE 2\n')
    links = run()
    assert len(links) == 9 and links[-1][links[-1].index('-o')+1].endswith('libstdc++.so')
    links = run('NB_GUEST_OPT=-O2 -fno-strict-aliasing')
    assert len(links) == 16 and all('-O2' in args and '-fno-strict-aliasing' in args for args in links[-7:])
    assert len(run('NB_GUEST_OPT=-O2 -fno-strict-aliasing')) == 16
    assert len(run()) == 23, 'returning to baseline flags did not invalidate release outputs'
    print(json.dumps({'status':'PASS', 'default':'historical compiler default', 'release':'explicit opt-in', 'libraries':7, 'checks':['unchanged stamp incremental','flag transitions both directions','JNI header dependency','standalone C++ header dependency','stamp excluded from link arguments','C++ fno-exceptions preserved'], 'guest_execution':False, 'runtime_build':False}, indent=2))
