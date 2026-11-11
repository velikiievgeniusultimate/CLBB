#!/usr/bin/env python3
"""Production ARM64 thunks: real array texture readback and scalar ABI banks."""
import os
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="clbb-stack-") as temporary:
    out = pathlib.Path(temporary)
    (out / "log").mkdir()
    (out / "log/log.h").write_text('#define LOG_ALWAYS_FATAL(...) __builtin_trap()\n')
    (out / "QemuInclude.h").write_text('''#include <stdint.h>
typedef uint64_t abi_ulong;
typedef struct { uint64_t xregs[32]; uint64_t pc; struct { __uint128_t zregs[32]; } vfp; } CPUARMState;
static inline void *g2h(uint64_t p) { return (void *)(uintptr_t)p; }
static inline uint64_t h2g_nocheck(void *p) { return (uintptr_t)p; }
''')
    production = (root / "src/nb-qemu/libnb-qemu/libGLESv2.def").read_text().splitlines()
    selected = [line for line in production if any(line.startswith('void ' + name + ' ')
                for name in ('glTexImage3D', 'glTexSubImage3D', 'glBlitFramebuffer'))]
    assert len(selected) == 3
    (out / "api.h").write_text('''#include <GLES3/gl3.h>
#include <stdint.h>
void integer_stack(uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint8_t,int16_t,uint32_t,uint64_t);
void mixed_banks(uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,uint32_t,float,uint32_t,float,float,float,float,float,float,float,float,uint8_t);
void floating_stack(double,double,double,double,double,double,double,double,float,double,uint32_t);
''')
    (out / "api.def").write_text('%include "api.h"\n' + '\n'.join(selected) + '''
void integer_stack (uint32_t a0, uint32_t a1, uint32_t a2, uint32_t a3, uint32_t a4, uint32_t a5, uint32_t a6, uint32_t a7, uint8_t a8, int16_t a9, uint32_t a10, uint64_t a11)
void mixed_banks (uint32_t a0, uint32_t a1, uint32_t a2, uint32_t a3, uint32_t a4, uint32_t a5, uint32_t a6, uint32_t a7, float f0, uint32_t a8, float f1, float f2, float f3, float f4, float f5, float f6, float f7, float f8, uint8_t a9)
void floating_stack (double f0, double f1, double f2, double f3, double f4, double f5, double f6, double f7, float f8, double f9, uint32_t a0)
''')
    generated = subprocess.check_output(['python3', str(root / 'src/nb-qemu/genlib.py'), str(out / 'api.def')])
    (out / 'generated.c').write_bytes(generated)
    (out / 'check.c').write_text(r'''
#include <EGL/egl.h>
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "generated.c"
static unsigned completed;
void integer_stack(uint32_t a0,uint32_t a1,uint32_t a2,uint32_t a3,uint32_t a4,uint32_t a5,uint32_t a6,uint32_t a7,uint8_t a8,int16_t a9,uint32_t a10,uint64_t a11) {
    uint32_t first[]={a0,a1,a2,a3,a4,a5,a6,a7};for(int i=0;i<8;i++)assert(first[i]==100u+i);
    assert(a8==231 && a9==-1234 && a10==0x12345678 && a11==UINT64_C(0xabcdef0123456789));completed++;
}
void mixed_banks(uint32_t a0,uint32_t a1,uint32_t a2,uint32_t a3,uint32_t a4,uint32_t a5,uint32_t a6,uint32_t a7,float f0,uint32_t a8,float f1,float f2,float f3,float f4,float f5,float f6,float f7,float f8,uint8_t a9) {
    uint32_t first[]={a0,a1,a2,a3,a4,a5,a6,a7};for(int i=0;i<8;i++)assert(first[i]==200u+i);
    float floats[]={f0,f1,f2,f3,f4,f5,f6,f7,f8};for(int i=0;i<9;i++)assert(floats[i]==i+.25f);
    assert(a8==208 && a9==209);completed++;
}
void floating_stack(double f0,double f1,double f2,double f3,double f4,double f5,double f6,double f7,float f8,double f9,uint32_t a0) {
    double first[]={f0,f1,f2,f3,f4,f5,f6,f7,f8,f9};for(int i=0;i<10;i++)assert(first[i]==i+.5);
    assert(a0==999);completed++;
}
static void set_float(CPUARMState *e,unsigned i,float v){memcpy(&e->vfp.zregs[i],&v,sizeof v);}
static void set_double(CPUARMState *e,unsigned i,double v){memcpy(&e->vfp.zregs[i],&v,sizeof v);}
static void graphics(void) {
    EGLDisplay display=eglGetDisplay(EGL_DEFAULT_DISPLAY);assert(eglInitialize(display,NULL,NULL));assert(eglBindAPI(EGL_OPENGL_ES_API));
    EGLint attributes[]={EGL_SURFACE_TYPE,EGL_PBUFFER_BIT,EGL_RENDERABLE_TYPE,EGL_OPENGL_ES2_BIT,EGL_NONE};EGLConfig config;EGLint n;
    assert(eglChooseConfig(display,attributes,&config,1,&n)&&n==1);
    EGLint context_attributes[]={EGL_CONTEXT_CLIENT_VERSION,3,EGL_NONE};EGLContext context=eglCreateContext(display,config,EGL_NO_CONTEXT,context_attributes);assert(context!=EGL_NO_CONTEXT);
    EGLint surface_attributes[]={EGL_WIDTH,2,EGL_HEIGHT,2,EGL_NONE};EGLSurface surface=eglCreatePbufferSurface(display,config,surface_attributes);assert(surface!=EGL_NO_SURFACE);
    assert(eglMakeCurrent(display,surface,surface,context));
    GLuint texture;glGenTextures(1,&texture);glBindTexture(GL_TEXTURE_2D_ARRAY,texture);
    CPUARMState env={0};uint64_t stack[8];memset(stack,0x55,sizeof stack);env.xregs[31]=(uintptr_t)stack;
    uint64_t args[]={GL_TEXTURE_2D_ARRAY,0,GL_RGBA8,2,2,2,0,GL_RGBA};memcpy(env.xregs,args,sizeof args);
    stack[0]=GL_UNSIGNED_BYTE;stack[1]=0;nb_handle_glTexImage3D(&env);assert(glGetError()==GL_NO_ERROR);
    unsigned char pixels[]={255,11,23,255,17,255,29,255,31,37,255,255,255,41,255,255};
    uint64_t upload[]={GL_TEXTURE_2D_ARRAY,0,0,0,1,2,2,1};memcpy(env.xregs,upload,sizeof upload);
    memset(stack,0x55,sizeof stack);stack[0]=UINT64_C(0x5555555500000000)|GL_RGBA;stack[1]=GL_UNSIGNED_BYTE;stack[2]=(uintptr_t)pixels;
    nb_handle_glTexSubImage3D(&env);GLenum error=glGetError();
    if(error){fprintf(stderr,"GENERATED_STACK_FAIL glTexSubImage3D error=%#x (ARM64 stack slots must advance by8)\n",error);exit(1);}
    GLuint fbo;glGenFramebuffers(1,&fbo);glBindFramebuffer(GL_FRAMEBUFFER,fbo);glFramebufferTextureLayer(GL_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,texture,0,1);
    assert(glCheckFramebufferStatus(GL_FRAMEBUFFER)==GL_FRAMEBUFFER_COMPLETE);unsigned char result[16];glReadPixels(0,0,2,2,GL_RGBA,GL_UNSIGNED_BYTE,result);
    assert(glGetError()==GL_NO_ERROR);assert(!memcmp(result,pixels,sizeof pixels));
    GLuint destination,destination_fbo;glGenTextures(1,&destination);glBindTexture(GL_TEXTURE_2D,destination);glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA8,2,2,0,GL_RGBA,GL_UNSIGNED_BYTE,NULL);
    glGenFramebuffers(1,&destination_fbo);glBindFramebuffer(GL_DRAW_FRAMEBUFFER,destination_fbo);glFramebufferTexture2D(GL_DRAW_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,destination,0);
    assert(glCheckFramebufferStatus(GL_DRAW_FRAMEBUFFER)==GL_FRAMEBUFFER_COMPLETE);
    uint64_t rectangle[]={0,0,2,2,0,0,2,2};memcpy(env.xregs,rectangle,sizeof rectangle);memset(stack,0x55,sizeof stack);
    stack[0]=UINT64_C(0x5555555500000000)|GL_COLOR_BUFFER_BIT;stack[1]=GL_NEAREST;nb_handle_glBlitFramebuffer(&env);assert(glGetError()==GL_NO_ERROR);
    glBindFramebuffer(GL_READ_FRAMEBUFFER,destination_fbo);memset(result,0,sizeof result);glReadPixels(0,0,2,2,GL_RGBA,GL_UNSIGNED_BYTE,result);
    assert(glGetError()==GL_NO_ERROR);assert(!memcmp(result,pixels,sizeof pixels));glDeleteFramebuffers(1,&destination_fbo);glDeleteTextures(1,&destination);
    glDeleteFramebuffers(1,&fbo);glDeleteTextures(1,&texture);eglMakeCurrent(display,EGL_NO_SURFACE,EGL_NO_SURFACE,EGL_NO_CONTEXT);eglDestroySurface(display,surface);eglDestroyContext(display,context);eglTerminate(display);
}
int main(void) {
    graphics();CPUARMState env={0};uint64_t stack[8];memset(stack,0x55,sizeof stack);env.xregs[31]=(uintptr_t)stack;
    for(unsigned i=0;i<8;i++)env.xregs[i]=100+i;
    stack[0]=231;stack[1]=(uint16_t)-1234;stack[2]=0x12345678;stack[3]=UINT64_C(0xabcdef0123456789);nb_handle_integer_stack(&env);
    for(unsigned i=0;i<8;i++){env.xregs[i]=200+i;set_float(&env,i,i+.25f);}
    stack[0]=208;float ninth=8.25f;memcpy(&stack[1],&ninth,sizeof ninth);stack[2]=209;nb_handle_mixed_banks(&env);
    for(unsigned i=0;i<8;i++)set_double(&env,i,i+.5);
    ninth=8.5f;double tenth=9.5;memcpy(&stack[0],&ninth,sizeof ninth);memcpy(&stack[1],&tenth,sizeof tenth);env.xregs[0]=999;nb_handle_floating_stack(&env);
    assert(completed==3);puts("GENERATED_STACK_OK array_texture_readback=1 framebuffer_blit_readback=1 scalar_stack_stride=8 independent_integer_float_banks=1");
}
''')
    flags = subprocess.check_output(['pkg-config', '--cflags', '--libs', 'egl', 'glesv2'], text=True).split()
    binary = out / 'check'
    subprocess.run(['cc', '-O1', '-g', '-fno-strict-aliasing', '-fsanitize=address,undefined',
                    '-I' + str(out), str(out / 'check.c'), '-o', str(binary), *flags], check=True)
    subprocess.run([str(binary)], env={**os.environ, 'EGL_PLATFORM': 'surfaceless',
                    'LIBGL_ALWAYS_SOFTWARE': '1', 'ASAN_OPTIONS': 'detect_leaks=0'}, check=True)
