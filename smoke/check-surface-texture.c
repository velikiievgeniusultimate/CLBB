#include <epoxy/gl.h>
#include <epoxy/egl.h>
#include <libavcodec/avcodec.h>
#include <glib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../src/atl/src/api-impl-jni/graphics/atl_surface_texture.h"
static void check(int ok, const char *what) { if (!ok) { fprintf(stderr,"SURFACE_TEXTURE_FAILURE %s gl=%#x egl=%#x\n",what,glGetError(),eglGetError());exit(1); } }
static GLuint shader(GLenum type, const char *text) {
    GLuint s=glCreateShader(type);glShaderSource(s,1,&text,NULL);glCompileShader(s);
    GLint ok=0;glGetShaderiv(s,GL_COMPILE_STATUS,&ok);if(!ok){char log[1024];glGetShaderInfoLog(s,sizeof log,NULL,log);fprintf(stderr,"%s\n",log);}check(ok,"shader compile");return s;
}
static AVFrame *rgba_frame(void) {
    AVFrame *f=av_frame_alloc();f->format=AV_PIX_FMT_RGBA;f->width=2;f->height=2;f->pts=1234;
    check(av_frame_get_buffer(f,0)==0,"allocate frame");
    unsigned char first[]={255,0,0,255,0,255,0,255},second[]={0,0,255,255,255,255,255,255};
    memcpy(f->data[0],first,8);memcpy(f->data[0]+f->linesize[0],second,8);return f;
}
static AVFrame *decoded_frame(const char *path) {
    gchar *contents=NULL;gsize n=0;check(g_file_get_contents(path,&contents,&n,NULL),"read h264 fixture");
    const AVCodec *codec=avcodec_find_decoder(AV_CODEC_ID_H264);AVCodecContext *c=avcodec_alloc_context3(codec);
    check(avcodec_open2(c,codec,NULL)==0,"open decoder");AVPacket *p=av_packet_alloc();check(av_new_packet(p,n)==0,"encoded packet");memcpy(p->data,contents,n);g_free(contents);p->pts=9000;
    check(avcodec_send_packet(c,p)==0,"decode packet");av_packet_free(&p);AVFrame *f=av_frame_alloc();
    int status=avcodec_receive_frame(c,f);if(status==AVERROR(EAGAIN)){check(avcodec_send_packet(c,NULL)==0,"drain decoder");status=avcodec_receive_frame(c,f);}check(status==0,"decoded frame");avcodec_free_context(&c);return f;
}
int main(int argc,char **argv) {
    check(argc==3,"arguments");int version=atoi(argv[1]);
    EGLDisplay d=eglGetDisplay(EGL_DEFAULT_DISPLAY);check(d!=EGL_NO_DISPLAY&&eglInitialize(d,NULL,NULL),"egl initialize");check(eglBindAPI(EGL_OPENGL_ES_API),"bind es");
    EGLint cfg[]={EGL_SURFACE_TYPE,EGL_PBUFFER_BIT,EGL_RENDERABLE_TYPE,EGL_OPENGL_ES2_BIT,EGL_RED_SIZE,8,EGL_GREEN_SIZE,8,EGL_BLUE_SIZE,8,EGL_NONE};EGLConfig config;EGLint count;
    check(eglChooseConfig(d,cfg,&config,1,&count)&&count==1,"config");EGLint ca[]={EGL_CONTEXT_CLIENT_VERSION,version,EGL_NONE};EGLContext context=eglCreateContext(d,config,EGL_NO_CONTEXT,ca);check(context!=EGL_NO_CONTEXT,"context");
    EGLint pa[]={EGL_WIDTH,2,EGL_HEIGHT,2,EGL_NONE};EGLSurface surface=eglCreatePbufferSurface(d,config,pa);check(surface!=EGL_NO_SURFACE&&eglMakeCurrent(d,surface,surface,context),"make current");
    GLuint names[4];glGenTextures(4,names);glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,names[0]);glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA,2,2,0,GL_RGBA,GL_UNSIGNED_BYTE,NULL);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_NEAREST);
    GLuint framebuffer;glGenFramebuffers(1,&framebuffer);glBindFramebuffer(GL_FRAMEBUFFER,framebuffer);glFramebufferTexture2D(GL_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,names[0],0);check(glCheckFramebufferStatus(GL_FRAMEBUFFER)==GL_FRAMEBUFFER_COMPLETE,"framebuffer");
    glActiveTexture(GL_TEXTURE3);glBindTexture(GL_TEXTURE_2D,names[1]);glBindTexture(GL_TEXTURE_EXTERNAL_OES,names[2]);glPixelStorei(GL_UNPACK_ALIGNMENT,8);
    gboolean extended=epoxy_gl_version()>=30;GLuint unpack=0;
    if(extended){glGenBuffers(1,&unpack);glBindBuffer(GL_PIXEL_UNPACK_BUFFER,unpack);glBufferData(GL_PIXEL_UNPACK_BUFFER,128,NULL,GL_STATIC_DRAW);glPixelStorei(GL_UNPACK_ROW_LENGTH,7);glPixelStorei(GL_UNPACK_SKIP_ROWS,2);glPixelStorei(GL_UNPACK_SKIP_PIXELS,1);}
    AtlTextureConsumer *consumer=atl_texture_consumer_new(names[3]);struct SwsContext *conversion=NULL;AVFrame *frame=rgba_frame();
    check(atl_texture_consumer_timestamp(consumer)==0,"empty timestamp");check(atl_texture_consumer_queue(consumer,frame,&conversion),"queue rgba");check(atl_texture_consumer_timestamp(consumer)==0,"timestamp only latches with image");
    GError *error=NULL;check(atl_texture_consumer_latch(consumer,&error),error?error->message:"latch");check(atl_texture_consumer_timestamp(consumer)==1234000,"pts nanoseconds");
    GLint value;glGetIntegerv(GL_ACTIVE_TEXTURE,&value);check(value==GL_TEXTURE3,"active texture preserved");glGetIntegerv(GL_TEXTURE_BINDING_2D,&value);check(value==(GLint)names[1],"2d binding preserved");glGetIntegerv(GL_TEXTURE_BINDING_EXTERNAL_OES,&value);check(value==(GLint)names[2],"external binding preserved");glGetIntegerv(GL_UNPACK_ALIGNMENT,&value);check(value==8,"alignment preserved");
    if(extended){glGetIntegerv(GL_PIXEL_UNPACK_BUFFER_BINDING,&value);check(value==(GLint)unpack,"unpack buffer preserved");glGetIntegerv(GL_UNPACK_ROW_LENGTH,&value);check(value==7,"row length preserved");glGetIntegerv(GL_UNPACK_SKIP_ROWS,&value);check(value==2,"skip rows preserved");glGetIntegerv(GL_UNPACK_SKIP_PIXELS,&value);check(value==1,"skip pixels preserved");}
    check(glGetError()==GL_NO_ERROR,"upload introduced GL error");
    GLuint vs=shader(GL_VERTEX_SHADER,"attribute vec2 p;varying vec2 uv;void main(){gl_Position=vec4(p,0.,1.);uv=(p+1.)*.5;}");
    GLuint fs=shader(GL_FRAGMENT_SHADER,"#extension GL_OES_EGL_image_external : require\nprecision mediump float;uniform samplerExternalOES image;varying vec2 uv;void main(){gl_FragColor=texture2D(image,uv);}");
    GLuint program=glCreateProgram();glAttachShader(program,vs);glAttachShader(program,fs);glBindAttribLocation(program,0,"p");glLinkProgram(program);glGetProgramiv(program,GL_LINK_STATUS,&value);check(value,"program link");glUseProgram(program);glUniform1i(glGetUniformLocation(program,"image"),3);
    float vertices[]={-1,-1,3,-1,-1,3};GLuint vbo;glGenBuffers(1,&vbo);glBindBuffer(GL_ARRAY_BUFFER,vbo);glBufferData(GL_ARRAY_BUFFER,sizeof vertices,vertices,GL_STATIC_DRAW);glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,0,NULL);glEnableVertexAttribArray(0);glViewport(0,0,2,2);glBindTexture(GL_TEXTURE_EXTERNAL_OES,names[3]);glDrawArrays(GL_TRIANGLES,0,3);
    unsigned char pixels[16];glReadPixels(0,0,2,2,GL_RGBA,GL_UNSIGNED_BYTE,pixels);unsigned char expected[]={255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255};check(memcmp(pixels,expected,16)==0,"actual external sampler pixels");
    av_frame_free(&frame);frame=decoded_frame(argv[2]);check(atl_texture_consumer_queue(consumer,frame,&conversion),"queue decoded h264");check(atl_texture_consumer_latch(consumer,&error),"latch h264");glBindTexture(GL_TEXTURE_EXTERNAL_OES,names[3]);glDrawArrays(GL_TRIANGLES,0,3);glReadPixels(0,0,2,2,GL_RGBA,GL_UNSIGNED_BYTE,pixels);
    for(int i=0;i<16;i+=4)check(pixels[i]>235&&pixels[i+1]<20&&pixels[i+2]<20&&pixels[i+3]==255,"decoded h264 pixels");
    frame->pts=10000;check(atl_texture_consumer_queue(consumer,frame,&conversion),"queue latest1");frame->pts=11000;check(atl_texture_consumer_queue(consumer,frame,&conversion),"queue latest2");check(atl_texture_consumer_latch(consumer,&error),"latch latest");check(atl_texture_consumer_timestamp(consumer)==11000000,"latest wins");
    atl_texture_consumer_ref(consumer);atl_texture_consumer_close(consumer);atl_texture_consumer_unref(consumer);check(!atl_texture_consumer_queue(consumer,frame,&conversion),"released consumer rejects producer safely");atl_texture_consumer_unref(consumer);av_frame_free(&frame);sws_freeContext(conversion);
    check(glGetError()==GL_NO_ERROR,"final GL status");printf("SURFACE_TEXTURE_OK es=%d rgba_pixels=1 h264_pixels=1 state_preserved=1 latest_queue=1 release_safe=1\n",version);
    glDeleteBuffers(1,&vbo);if(unpack)glDeleteBuffers(1,&unpack);glDeleteProgram(program);glDeleteShader(vs);glDeleteShader(fs);glDeleteFramebuffers(1,&framebuffer);glDeleteTextures(4,names);eglMakeCurrent(d,EGL_NO_SURFACE,EGL_NO_SURFACE,EGL_NO_CONTEXT);eglDestroyContext(d,context);eglDestroySurface(d,surface);eglTerminate(d);return 0;
}
