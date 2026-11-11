#!/usr/bin/env python3
"""Read back real Mesa pixels through an already-built GLES thunk artifact.

No game process, desktop display, focus or installation is involved. Uses the
production CPUARMState layout and the artifact's exported SVC dispatcher.
"""
import argparse
import ast
import os
from pathlib import Path
import re
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('artifact', nargs='?', default=str(root/'src/nb-qemu/builddir-release-ab/libnb-qemu'))
args = parser.parse_args()
artifact = Path(args.artifact).resolve()
nb = root/'src/nb-qemu/libnb-qemu'
qemu = root/'src/nb-qemu/qemu'
# Reuse the established independent array-texture/blit oracle; execute it
# against the actual shared library, rather than compiling another thunk copy.
tree = ast.parse((root/'smoke/check-generated-stack.py').read_text())
body = next(node.args[0].value for node in ast.walk(tree) if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute) and node.func.attr == 'write_text'
            and node.args and isinstance(node.args[0], ast.Constant)
            and isinstance(node.args[0].value, str) and 'static void graphics(void)' in node.args[0].value)
start = body.index('static void graphics(void)')
opening = body.index('{', start)
end, depth = opening+1, 1
while depth:
    depth += (body[end] == '{') - (body[end] == '}')
    end += 1
graphics = body[start:end]
generated = (artifact/'libGLESv2.c').read_text()
for name in ['glTexImage3D', 'glTexSubImage3D', 'glBlitFramebuffer']:
    code = re.search(r'case (0x[0-9a-f]+): nb_handle_'+name+r'\(env\)', generated)[1]
    graphics = graphics.replace('nb_handle_'+name+'(&env)', 'dispatch(&env,'+code+')')
fixture = r'''
#define _GNU_SOURCE
#include "QemuInclude.h"
#include <EGL/egl.h>
#include <GLES3/gl3.h>
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void (*dispatch)(CPUARMState *, int);
/* These default-FBO functions normally come from ATL's native GLES adapter.
 * The fixture uses only ordinary offscreen Mesa FBOs, so direct forwarding is
 * its complete host adapter. The production thunk calls the real functions. */
void bionic_glBindFramebuffer(GLenum t, GLuint f){glBindFramebuffer(t,f);}
void bionic_glGetIntegerv(GLenum p, GLint *v){glGetIntegerv(p,v);}
'''+graphics+r'''
int main(int argc,char **argv){
 assert(argc==2);
 void *library=dlopen(argv[1],RTLD_LAZY|RTLD_LOCAL);
 if(!library){fprintf(stderr,"THUNK_LOAD_FAIL %s\n",dlerror());return 1;}
 dispatch=dlsym(library,"nb_handle_svc");assert(dispatch);
 graphics();
 puts("RELEASE_THUNK_OK actual_shared_library=1 production_cpu_layout=1 array_texture_pixels=1 framebuffer_blit_pixels=1");
 return 0;
}
'''
with tempfile.TemporaryDirectory(prefix='clbb-release-thunk-') as tmp:
    directory = Path(tmp)
    source, binary = directory/'check.c', directory/'check'
    source.write_text(fixture)
    includes = [nb/'_includes', nb, qemu/'_build', qemu/'_build/aarch64-linux-user', qemu/'include',
                qemu/'linux-user', qemu/'linux-user/aarch64', qemu/'linux-user/host/x86_64', qemu/'tcg/i386', qemu/'target/arm']
    flags = subprocess.check_output(['pkg-config','--cflags','--libs','glib-2.0','capstone','egl','glesv2'], text=True).split()
    env = {**os.environ, 'EGL_PLATFORM':'surfaceless', 'LIBGL_ALWAYS_SOFTWARE':'1',
           'LD_LIBRARY_PATH':str(artifact)+':'+str(root/'prefix/lib')+':'+str(root/'prefix/lib/art')+':'+os.environ.get('LD_LIBRARY_PATH','')}
    subprocess.run(['cc','-O2','-g','-fno-strict-aliasing','-rdynamic',*["-I"+str(p) for p in includes],
                    str(source),'-o',str(binary),'-L'+str(artifact),'-lnb-qemu',*flags,'-ldl'],env=env,check=True)
    subprocess.run([str(binary),str(artifact/'libnb-qemu-GLESv2.so')], env=env, check=True, timeout=15)
