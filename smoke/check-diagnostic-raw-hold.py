#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check extracted production raw-hold parsing and contact/timer lifecycle.

Only a temporary C fixture is compiled. Native touch delivery and timers are
stubs: this test opens no runtime socket, desktop display, or game process.
The parser, hold-command branch and release callback come from clbb_mapper.c.
"""
from pathlib import Path
import re
import shlex
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/atl/src/main-executable/clbb_mapper.c"


def production_body(source: str, marker: str) -> str:
    if source.count(marker) != 1:
        raise RuntimeError(f"Expected exactly one production definition: {marker}")
    start = source.index(marker)
    opening = source.index("{", start)
    tokens = re.finditer(
        r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[{}]',
        source[opening:], re.S)
    depth = 0
    for token in tokens:
        if token.group() == "{":
            depth += 1
        elif token.group() == "}":
            depth -= 1
            if depth == 0:
                return source[start:opening + token.end()]
    raise RuntimeError(f"Unterminated production definition: {marker}")


PRELUDE = r'''
#include <glib.h>
#include <errno.h>
#include <math.h>
#include <assert.h>
#include <string.h>
#define GTK_WIDGET(x) (x)
typedef struct {
    void *window;
    gboolean raw_tap,closing;
    double raw_x,raw_y,raw_dx,raw_dy;
    unsigned raw_steps;
    guint raw_timer;
} ClbbHost;
static guint interval,downs,ups,moves;
static gboolean accept=TRUE;
static GSourceFunc saved_callback;
static gpointer saved_context;
static void *game_surface(void *window) { return window; }
static int clbb_view_touch(void *window,int edge,int finger,double x,double y) {
    assert(window==(void*)1 && finger==-1 && isfinite(x) && isfinite(y));
    if(edge==0) { ++downs;return accept; }
    if(edge==1) ++moves;
    else if(edge==2) ++ups;
    else assert(0);
    return 1;
}
static guint test_timeout_add(guint ms,GSourceFunc callback,gpointer context) {
    interval=ms;saved_callback=callback;saved_context=context;return 42;
}
#define g_timeout_add test_timeout_add
'''


TEST = r'''
static void reset(void) {
    interval=downs=ups=moves=0;saved_callback=NULL;saved_context=NULL;accept=TRUE;
}
static void parser_checks(void) {
    const char *good[][3]={
        {"0","1","16"},{"1","0","5000"},
        {"0.92","0.896","950"},{".5","0.5","800"}
    };
    for(unsigned i=0;i<G_N_ELEMENTS(good);++i) {
        double x=-1,y=-1;guint ms=0;
        assert(raw_hold_arguments(good[i][0],good[i][1],good[i][2],&x,&y,&ms));
        assert(x>=0&&x<=1&&y>=0&&y<=1&&ms>=16&&ms<=5000);
    }
    const char *bad[][3]={
        {"nan",".5","900"},{"inf",".5","900"},{".5","-inf","900"},
        {"1e309",".5","900"},{"-0.01",".5","900"},{"1.01",".5","900"},
        {".5","1.01","900"},{"",".5","900"},{".5junk",".5","900"},
        {".5",".5junk","900"},{".5",".5","15"},{".5",".5","5001"},
        {".5",".5","-16"},{".5",".5","+16"},{".5",".5","16.0"},
        {".5",".5","16x"},{".5",".5","184467440737095516160"},
        {".5",".5",""},{".5",".5","nan"}
    };
    for(unsigned i=0;i<G_N_ELEMENTS(bad);++i) {
        double x=-2,y=-3;guint ms=7;
        assert(!raw_hold_arguments(bad[i][0],bad[i][1],bad[i][2],&x,&y,&ms));
        assert(x==-2&&y==-3&&ms==7);
    }
}
int main(void) {
    parser_checks();
    char *valid[]={"hold","0.92","0.89","950",NULL};
    ClbbHost host={.window=(void*)1};reset();
    char *reply=hold(&host,valid);
    assert(strcmp(reply,"OK hold")==0);g_free(reply);
    assert(host.raw_tap&&host.raw_steps==0&&host.raw_timer==42&&interval==950);
    assert(downs==1&&ups==0&&moves==0&&saved_context==&host);
    assert(saved_callback(&host)==G_SOURCE_REMOVE);
    assert(ups==1&&!host.raw_tap&&host.raw_timer==0);
    // Neither an active raw contact nor its pending timer can be replaced.
    reset();host.raw_tap=TRUE;host.raw_timer=7;host.raw_x=.2;
    reply=hold(&host,valid);
    assert(strcmp(reply,"REJECTED hold")==0);g_free(reply);
    assert(downs==0&&interval==0&&host.raw_timer==7&&host.raw_x==.2);
    host.raw_tap=FALSE;reply=hold(&host,valid);
    assert(strcmp(reply,"REJECTED hold")==0);g_free(reply);
    assert(downs==0&&interval==0&&host.raw_timer==7);
    // A rejected native DOWN must not schedule an orphan release timer.
    host.raw_timer=0;reset();accept=FALSE;reply=hold(&host,valid);
    assert(strcmp(reply,"REJECTED hold")==0);g_free(reply);
    assert(downs==1&&interval==0&&!host.raw_tap&&!host.raw_timer);
    reset();char *bad[]={"hold","nan",".5","900",NULL};reply=hold(&host,bad);
    assert(strcmp(reply,"REJECTED hold")==0);g_free(reply);
    assert(downs==0&&interval==0);
    g_print("DIAGNOSTIC_RAW_HOLD_OK parser_valid=4 parser_invalid=19 "
            "timed_down_up=1 busy_rejected=1 failed_down_no_timer=1 invalid_no_input=1\n");
}
'''


def main():
    source = SOURCE.read_text()
    parser = production_body(source, "static gboolean raw_hold_arguments(")
    finish = production_body(source, "static gboolean finish_raw_tap(")
    branch = production_body(source, 'else if (count == 4 && strcmp(args[0], "hold") == 0)')
    branch = branch.removeprefix("else ")
    fixture = (PRELUDE + parser + "\n" + finish +
               "\nstatic char *hold(ClbbHost *h,char **args) { guint count=4;char *reply=NULL;\n" +
               branch + "\nreturn reply; }\n" + TEST)
    flags = shlex.split(subprocess.check_output(
        ["pkgconf", "--cflags", "--libs", "glib-2.0"], text=True))
    with tempfile.TemporaryDirectory(prefix="clbb-diagnostic-raw-hold-") as directory:
        c_source = Path(directory) / "check.c"
        executable = Path(directory) / "check"
        c_source.write_text(fixture)
        subprocess.run(["cc", "-Wall", "-Wextra", "-Werror", "-std=c11",
                        str(c_source), "-o", str(executable), *flags, "-lm"], check=True)
        subprocess.run([str(executable)], check=True)


if __name__ == "__main__":
    main()
