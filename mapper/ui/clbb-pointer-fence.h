/* CLBB-owned X11 confinement. Barriers survive toolkit pointer re-grabs and
 * implicit grab release after a click; an XGrabPointer alone does not. */
#ifndef CLBB_POINTER_FENCE_H
#define CLBB_POINTER_FENCE_H
#include <X11/Xlib.h>
#include <X11/extensions/Xfixes.h>
#include <string.h>
typedef struct {
    PointerBarrier edges[4];
    int x, y, width, height;
} ClbbPointerFence;
static void clbb_pointer_fence_clear(Display *d, ClbbPointerFence *f) {
    for (int i=0;i<4;i++) if(f->edges[i]) XFixesDestroyPointerBarrier(d,f->edges[i]);
    memset(f,0,sizeof(*f));
}
static int clbb_pointer_fence_update(Display *d, Window window,
        int x, int y, int width, int height, ClbbPointerFence *f) {
    static Display *checked_display;static int supported;
    if(checked_display!=d) {
        int major=5,minor=0,event,error;
        supported=XFixesQueryExtension(d,&event,&error) &&
            XFixesQueryVersion(d,&major,&minor) && major>=5;
        checked_display=d;
    }
    if(!supported || width<2 || height<2)return 0;
    Window root=DefaultRootWindow(d),child;XWindowAttributes root_attrs;int rx,ry;
    if(!XTranslateCoordinates(d,window,root,x,y,&rx,&ry,&child))return 0;
    if(f->edges[0] && f->x==rx && f->y==ry && f->width==width && f->height==height)return 1;
    if(!XGetWindowAttributes(d,root,&root_attrs))return 0;
    /* Install new edges before removing the old ones during a resize. */
    ClbbPointerFence next={.x=rx,.y=ry,.width=width,.height=height};
    /* Full root-span edges also stop large diagonal motion crossing both
     * finite rectangle segments outside their endpoints in a single event. */
    next.edges[0]=XFixesCreatePointerBarrier(d,root,rx,0,rx,root_attrs.height,BarrierPositiveX,0,NULL);
    next.edges[1]=XFixesCreatePointerBarrier(d,root,rx+width,0,rx+width,root_attrs.height,BarrierNegativeX,0,NULL);
    next.edges[2]=XFixesCreatePointerBarrier(d,root,0,ry,root_attrs.width,ry,BarrierPositiveY,0,NULL);
    next.edges[3]=XFixesCreatePointerBarrier(d,root,0,ry+height,root_attrs.width,ry+height,BarrierNegativeY,0,NULL);
    clbb_pointer_fence_clear(d,f);*f=next;XFlush(d);return 1;
}
#endif
