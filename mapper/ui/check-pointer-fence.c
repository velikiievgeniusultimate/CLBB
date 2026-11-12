/* Runs only on an isolated Xvfb, including synthetic pointer movement. */
#include "clbb-pointer-fence.h"
#include <X11/extensions/XTest.h>
#include <assert.h>
#include <stdio.h>
static void move(Display*d,int dx,int dy,int left,int top,int right,int bottom) {
    XTestFakeRelativeMotionEvent(d,dx,dy,CurrentTime);XSync(d,False);
    Window root,child;int x,y,wx,wy;unsigned mask;
    assert(XQueryPointer(d,DefaultRootWindow(d),&root,&child,&x,&y,&wx,&wy,&mask));
    fprintf(stderr,"move %d %d => %d %d bounds %d %d %d %d\n",dx,dy,x,y,left,top,right,bottom);
    assert(x>=left && x<right && y>=top && y<bottom);
}
int main(void) {
    Display*d=XOpenDisplay(NULL);assert(d);
    Window root=DefaultRootWindow(d),w=XCreateSimpleWindow(d,root,50,50,800,600,0,0,0);
    XMapWindow(d,w);XSync(d,False);
    ClbbPointerFence f={0};
    assert(clbb_pointer_fence_update(d,w,200,47,400,500,&f));
    XWarpPointer(d,None,root,0,0,0,0,400,300);XSync(d,False);
    // Simulate toolkit grabbing the WHOLE window then releasing on button UP.
    assert(XGrabPointer(d,w,True,0,GrabModeAsync,GrabModeAsync,w,None,CurrentTime)==GrabSuccess);
    XUngrabPointer(d,CurrentTime);XSync(d,False);
    move(d,-1500,0,250,97,650,597);move(d,1500,0,250,97,650,597);
    move(d,0,-1500,250,97,650,597);move(d,0,1500,250,97,650,597);
    move(d,-1500,-1500,250,97,650,597);
    // Window move and new geometry must not leave the old barriers behind.
    XMoveWindow(d,w,100,100);XSync(d,False);
    assert(clbb_pointer_fence_update(d,w,100,80,300,300,&f));
    XWarpPointer(d,None,root,0,0,0,0,300,300);XSync(d,False);
    move(d,1500,1500,200,180,500,480);move(d,-1500,-1500,200,180,500,480);
    clbb_pointer_fence_clear(d,&f);XSync(d,False);
    assert(!f.edges[0]);
    XTestFakeRelativeMotionEvent(d,1000,0,CurrentTime);XSync(d,False);
    Window r,c;int x,y,a,b;unsigned mask;XQueryPointer(d,root,&r,&c,&x,&y,&a,&b,&mask);assert(x>=500);
    XDestroyWindow(d,w);XCloseDisplay(d);puts("CLBB_POINTER_FENCE_OK toolkit_regrab resize move unlock");
}
