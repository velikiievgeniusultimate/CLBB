#!/usr/bin/env python3
"""Exercise production X11 confine/unlock on a private Xvfb only.

The C bodies are extracted from the native adapter, not reimplemented here.
GTK uses the real CLBB logical viewport; a second X client verifies the actual
server grab. No command or X input is sent to the user's DISPLAY.
"""
from pathlib import Path
import os
import re
import shlex
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/atl/src/main-executable/clbb_mapper.c"


def production_function(source: str, name: str) -> str:
    matches = list(re.finditer(r"static\s+(?:void|gboolean)\s+" + re.escape(name) + r"\s*\([^;{}]*\)\s*\{", source))
    if len(matches) != 1:
        raise RuntimeError(f"Expected exactly one production definition of {name}")
    start = matches[0].start()
    opening = matches[0].end() - 1
    # Braces inside comments/strings do not end a C function. Compile precisely
    # the complete original body, including its nested if blocks.
    tokens = re.finditer(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[{}]', source[opening:], re.S)
    depth = 0
    for token in tokens:
        if token.group() == "{":
            depth += 1
        elif token.group() == "}":
            depth -= 1
            if depth == 0:
                return source[start:opening + token.end()]
    raise RuntimeError(f"Unterminated production definition: {name}")


PRELUDE = r'''
#include <gtk/gtk.h>
#include <gdk/x11/gdkx.h>
#include <X11/Xlib.h>
#include <math.h>
#include <stdlib.h>
#include "clbb-viewport.h"
typedef struct {
    GtkWindow *window;
    Display *display;
    Window fence, fence_parent;
    GdkRectangle fence_rect;
    gboolean locked;
} ClbbHost;
static GtkWidget *test_game;
static GtkWidget *game_surface(GtkWidget *root) { (void)root; return test_game; }
static int resize_requests,grab_requests,warp_requests;
static int counted_resize(Display *d,Window w,int x,int y,unsigned width,unsigned height) {
    ++resize_requests;return XMoveResizeWindow(d,w,x,y,width,height);
}
static int counted_grab(Display *d,Window w,Bool owner,unsigned events,int pointer_mode,int keyboard_mode,Window fence,Cursor cursor,Time time) {
    ++grab_requests;return XGrabPointer(d,w,owner,events,pointer_mode,keyboard_mode,fence,cursor,time);
}
static int counted_warp(Display *d,Window source,Window dest,int sx,int sy,unsigned sw,unsigned sh,int dx,int dy) {
    ++warp_requests;return XWarpPointer(d,source,dest,sx,sy,sw,sh,dx,dy);
}
#define XMoveResizeWindow counted_resize
#define XGrabPointer counted_grab
#define XWarpPointer counted_warp
'''

TEST = r'''
#undef XMoveResizeWindow
#undef XGrabPointer
#undef XWarpPointer
static void wait_layout(GtkWidget *viewport, int width, int height) {
    const gint64 deadline=g_get_monotonic_time()+3000000;
    do {
        while(g_main_context_iteration(NULL,FALSE)) {}
        if(gtk_widget_get_mapped(test_game) &&
           gtk_widget_get_width(viewport)==width && gtk_widget_get_height(viewport)==height &&
           gtk_widget_get_width(test_game)==4000 && gtk_widget_get_height(test_game)==3111) return;
        g_usleep(1000);
    } while(g_get_monotonic_time()<deadline);
    g_error("Viewport did not receive bounded physical and fixed logical allocations");
}
static void warp(Display *display, Window window, int x, int y) {
    XWarpPointer(display,None,window,0,0,0,0,x,y);
    XSync(display,False);
}
static void query(Display *display, Window window, int *x, int *y) {
    Window root,child;int root_x,root_y;unsigned mask;
    g_assert_true(XQueryPointer(display,window,&root,&child,&root_x,&root_y,x,y,&mask));
}
static int second_client_grab(Display *other, Window xid) {
    int status=XGrabPointer(other,xid,True,PointerMotionMask,GrabModeAsync,GrabModeAsync,None,None,CurrentTime);
    XSync(other,False);
    return status;
}
static void check_mode(ClbbHost *host, GtkWidget *viewport, int width, int height) {
    wait_layout(viewport,width,height);
    GdkSurface *surface=gtk_native_get_surface(GTK_NATIVE(host->window));
    Window xid=gdk_x11_surface_get_xid(surface);
    Display *display=gdk_x11_display_get_xdisplay(gdk_surface_get_display(surface));
    Display *other=XOpenDisplay(DisplayString(display));
    g_assert_nonnull(other);
    graphene_rect_t content;
    g_assert_true(clbb_viewport_get_content_rect(viewport,&content));
    g_assert_cmpfloat(content.origin.x,>,1.);
    // A black side bar must never be accepted as the game picture.
    warp(display,xid,0,height/2);
    confine(host);
    g_assert_false(host->locked);g_assert_cmpuint(host->fence,==,0);
    warp(display,xid,width/2,height/2);
    confine(host);
    g_assert_true(host->locked);g_assert_cmpuint(host->fence,!=,0);
    g_assert_true(host->display==display);
    XSync(display,False);
    XWindowAttributes attrs;
    g_assert_true(XGetWindowAttributes(display,host->fence,&attrs));
    g_assert_cmpint(attrs.class,==,InputOnly);
    g_assert_cmpint(attrs.map_state,==,IsViewable);
    g_assert_cmpint(attrs.x,==,(int)content.origin.x);
    g_assert_cmpint(attrs.y,==,(int)content.origin.y);
    g_assert_cmpint(attrs.width,==,(int)content.size.width);
    g_assert_cmpint(attrs.height,==,(int)content.size.height);
    g_assert_cmpint(attrs.width,<=,width);g_assert_cmpint(attrs.height,<=,height);
    g_assert_cmpint(attrs.width,<,4000);g_assert_cmpint(attrs.height,<,3111);
    Window ignored;int fence_root_x,fence_root_y;
    g_assert_true(XTranslateCoordinates(display,host->fence,DefaultRootWindow(display),0,0,
                                        &fence_root_x,&fence_root_y,&ignored));
    for(int corner=0;corner<2;++corner) {
        warp(display,DefaultRootWindow(display),corner?1919:0,corner?1079:0);
        int root_x,root_y;query(display,DefaultRootWindow(display),&root_x,&root_y);
        g_assert_cmpint(root_x,>=,fence_root_x);
        g_assert_cmpint(root_y,>=,fence_root_y);
        g_assert_cmpint(root_x,<,fence_root_x+attrs.width);
        g_assert_cmpint(root_y,<,fence_root_y+attrs.height);
    }
    // Server evidence, after the movement assertions: this second-client
    // XGrabPointer(None) probe changes pointer limits in this Xvfb even when it
    // returns AlreadyGrabbed (also reproduced in bare Xlib). It must therefore
    // not precede the real confinement check above.
    g_assert_cmpint(second_client_grab(other,xid),==,AlreadyGrabbed);
    unlock(host);XSync(display,False);
    g_assert_false(host->locked);g_assert_cmpuint(host->fence,==,0);
    g_assert_cmpint(second_client_grab(other,xid),==,GrabSuccess);
    XUngrabPointer(other,CurrentTime);XSync(other,False);
    warp(display,DefaultRootWindow(display),0,0);
    int x,y;query(display,DefaultRootWindow(display),&x,&y);
    g_assert_cmpint(x,==,0);g_assert_cmpint(y,==,0);
    // Production F12 toggles off an existing lock rather than leaving an old
    // InputOnly fence behind. The second connection proves it was released.
    warp(display,xid,width/2,height/2);confine(host);g_assert_true(host->locked);
    confine(host);g_assert_false(host->locked);g_assert_cmpuint(host->fence,==,0);
    XSync(display,False);
    g_assert_cmpint(second_client_grab(other,xid),==,GrabSuccess);
    XUngrabPointer(other,CurrentTime);XSync(other,False);XCloseDisplay(other);
    g_print("CLBB_CONFINE_OK physical=%dx%d logical=4000x3111 fence=%d,%d %dx%d warp_clipped=1 unlock_verified=1\n",
            width,height,attrs.x,attrs.y,attrs.width,attrs.height);
}
static void check_active_resize(ClbbHost *host,GtkWidget *viewport,GtkWidget *stack) {
    GdkSurface *surface=gtk_native_get_surface(GTK_NATIVE(host->window));
    Window xid=gdk_x11_surface_get_xid(surface);
    Display *display=gdk_x11_display_get_xdisplay(gdk_surface_get_display(surface));
    warp(display,xid,960,540);confine(host);g_assert_true(host->locked);
    const Window fence=host->fence;const int grabs=grab_requests,warps=warp_requests;
    int moves=resize_requests;
    refresh_confine(host);refresh_confine(host);g_assert_cmpint(resize_requests,==,moves);
    gtk_window_set_default_size(host->window,800,600);wait_layout(viewport,800,600);
    refresh_confine(host);XSync(display,False);
    g_assert_true(host->locked);g_assert_cmpuint(host->fence,==,fence);
    g_assert_cmpint(resize_requests,==,moves+1);++moves;
    g_assert_cmpint(grab_requests,==,grabs);g_assert_cmpint(warp_requests,==,warps);
    XWindowAttributes attrs;g_assert_true(XGetWindowAttributes(display,fence,&attrs));
    g_assert_cmpint(attrs.x,==,14);g_assert_cmpint(attrs.y,==,0);
    g_assert_cmpint(attrs.width,==,771);g_assert_cmpint(attrs.height,==,600);
    refresh_confine(host);refresh_confine(host);g_assert_cmpint(resize_requests,==,moves);
    // A newly shown activity has a different, inset SurfaceView. Its physical
    // geometry must update the same existing fence without grabbing again.
    GtkWidget *activity=gtk_fixed_new(),*replacement=gtk_drawing_area_new();
    gtk_widget_set_size_request(replacement,2000,1555);
    gtk_fixed_put(GTK_FIXED(activity),replacement,1000,700);
    gtk_stack_add_named(GTK_STACK(stack),activity,"replacement-activity");
    gtk_stack_set_visible_child(GTK_STACK(stack),activity);
    test_game=replacement;
    const gint64 deadline=g_get_monotonic_time()+3000000;
    while(!gtk_widget_get_mapped(replacement)||gtk_widget_get_width(replacement)!=2000) {
        while(g_main_context_iteration(NULL,FALSE)) {}
        if(g_get_monotonic_time()>deadline)g_error("Replacement activity did not allocate");
        g_usleep(1000);
    }
    refresh_confine(host);XSync(display,False);
    g_assert_true(host->locked);g_assert_cmpuint(host->fence,==,fence);
    g_assert_cmpint(resize_requests,==,moves+1);++moves;
    g_assert_cmpint(grab_requests,==,grabs);g_assert_cmpint(warp_requests,==,warps);
    g_assert_true(XGetWindowAttributes(display,fence,&attrs));
    g_assert_cmpint(attrs.x,>,100);g_assert_cmpint(attrs.y,>,100);
    g_assert_cmpint(attrs.width,>,350);g_assert_cmpint(attrs.width,<,400);
    g_assert_cmpint(attrs.height,>,280);g_assert_cmpint(attrs.height,<,320);
    refresh_confine(host);g_assert_cmpint(resize_requests,==,moves);
    warp(display,DefaultRootWindow(display),0,0);
    int root_x,root_y,origin_x,origin_y;Window ignored;
    g_assert_true(XTranslateCoordinates(display,fence,DefaultRootWindow(display),0,0,&origin_x,&origin_y,&ignored));
    query(display,DefaultRootWindow(display),&root_x,&root_y);
    g_assert_cmpint(root_x,>=,origin_x);g_assert_cmpint(root_x,<,origin_x+attrs.width);
    g_assert_cmpint(root_y,>=,origin_y);g_assert_cmpint(root_y,<,origin_y+attrs.height);
    test_game=NULL;refresh_confine(host);XSync(display,False);
    g_assert_false(host->locked);g_assert_cmpuint(host->fence,==,0);
    Display *other=XOpenDisplay(DisplayString(display));g_assert_nonnull(other);
    g_assert_cmpint(second_client_grab(other,xid),==,GrabSuccess);
    XUngrabPointer(other,CurrentTime);XSync(other,False);XCloseDisplay(other);
    g_print("CLBB_CONFINE_RESIZE_OK same_fence=1 geometry_updates=2 unchanged_skipped=1 no_regrab=1 no_warp=1 surface_loss_unlock=1\n");
}
int main(void) {
    // A hard guard in the binary as well as a fresh Xvfb in the Python runner.
    // In particular, no XOpenDisplay/XWarpPointer may ever target DISPLAY :1.
    const char *display=getenv("DISPLAY");
    if(!getenv("CLBB_CONFINE_PRIVATE_XVFB") || !display || display[0]!=':' || atoi(display+1)<190)
        g_error("Refusing confinement test outside its private Xvfb display");
    (void)counted_warp;
    gtk_init();
    GtkWidget *stack=gtk_stack_new();test_game=gtk_drawing_area_new();
    gtk_widget_set_size_request(test_game,4000,3111);
    gtk_widget_set_hexpand(test_game,TRUE);gtk_widget_set_vexpand(test_game,TRUE);
    gtk_stack_add_named(GTK_STACK(stack),test_game,"android-activity");
    GtkWidget *viewport=clbb_viewport_new(stack,4000,3111);
    GtkWidget *window=gtk_window_new();gtk_window_set_decorated(GTK_WINDOW(window),FALSE);
    gtk_window_set_child(GTK_WINDOW(window),viewport);
    gtk_window_set_default_size(GTK_WINDOW(window),800,600);
    gtk_window_present(GTK_WINDOW(window));ClbbHost host={.window=GTK_WINDOW(window)};
    check_mode(&host,viewport,800,600);
    gtk_window_set_default_size(GTK_WINDOW(window),1920,1080);
    check_mode(&host,viewport,1920,1080);
    check_active_resize(&host,viewport,stack);
    unlock(&host);gtk_window_destroy(GTK_WINDOW(window));
    while(g_main_context_iteration(NULL,FALSE)) {}
    return 0;
}
'''


def main():
    source = SOURCE.read_text()
    with tempfile.TemporaryDirectory(prefix="clbb-confine-") as temporary:
        path = Path(temporary)
        c_file = path / "check.c"
        c_file.write_text(PRELUDE + "\n" + "\n".join(production_function(source,name) for name in ["fence_geometry","unlock","refresh_confine","confine"]) + TEST)
        flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "gtk4", "x11"], text=True))
        subprocess.run(["cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-Wno-deprecated-declarations", str(c_file), str(ROOT / "mapper/ui/clbb-viewport.c"), "-I", str(ROOT / "mapper/ui"), *flags, "-lm", "-o", str(path / "check")], check=True)
        # xvfb-run creates its own Xauthority and terminates the server on exit.
        # Starting at 190 excludes the user's :1 even if inherited in os.environ.
        environment = dict(os.environ, GDK_BACKEND="x11", GSK_RENDERER="cairo", GTK_A11Y="none", CLBB_CONFINE_PRIVATE_XVFB="1")
        command = ["xvfb-run", "--auto-servernum", "--server-num=190", "--server-args=-screen 0 1920x1080x24 -nolisten tcp", str(path / "check")]
        process = subprocess.Popen(command, env=environment, start_new_session=True)
        try:
            result = process.wait(timeout=20)
        except BaseException:
            # Terminate the owned process group, including the X server, even
            # when compilation/test infrastructure interrupts this runner.
            try:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=3)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            raise
        if result:
            raise subprocess.CalledProcessError(result, command)


if __name__ == "__main__":
    main()
