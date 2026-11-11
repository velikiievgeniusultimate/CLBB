#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include "webview_diagnostics.h"
static GMainLoop *loop;
static gboolean done(gpointer data) {(void)data;g_main_loop_quit(loop);return G_SOURCE_REMOVE;}
int main(void)
{
    if (getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init();loop=g_main_loop_new(NULL,FALSE);
    WebKitWebContext *context=webkit_web_context_new();
    const char *prefix=getenv("CLBB_WEBKIT_PROCESS_DIR");if(prefix)webkit_web_context_add_path_to_sandbox(context,prefix,TRUE);
    WebKitNetworkSession *session=webkit_network_session_new_ephemeral();
    WebKitWebView *view=WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,"web-context",context,"network-session",session,NULL));
    atl_webview_diagnostics_install(view);atl_webview_diagnostics_install(view);
    GtkWidget *window=gtk_window_new();gtk_window_set_default_size(GTK_WINDOW(window),400,200);
    gtk_window_set_child(GTK_WINDOW(window),GTK_WIDGET(view));gtk_window_present(GTK_WINDOW(window));
    const char *html="<!doctype html><script>setTimeout(()=>{"
        "dispatchEvent(new ErrorEvent('error',{error:new ReferenceError('LOCAL_PRIVATE_SENTINEL is not defined')}));"
        "Promise.reject(new TypeError('LOCAL_PRIVATE_SENTINEL is not a function'));"
        "window.webkit.messageHandlers.atlWebDiagnostics.postMessage(JSON.stringify({kind:'error',type:'LOCAL_PRIVATE_SENTINEL',storage:'LOCAL_PRIVATE_SENTINEL',network:1}));"
        "window.webkit.messageHandlers.atlWebDiagnostics.postMessage(JSON.stringify({kind:'csp',directive:'LOCAL_PRIVATE_SENTINEL'}));"
        "window.webkit.messageHandlers.atlWebDiagnostics.postMessage('invalid LOCAL_PRIVATE_SENTINEL');"
        "},0);</script>";
    webkit_web_view_load_html(view,html,"http://localhost/");
    atl_webview_diagnostics_cookie_count(view,"mtacc.mobilelegends.com");
    g_timeout_add_seconds(2,done,NULL);g_main_loop_run(loop);
    atl_webview_diagnostics_remove(view);atl_webview_diagnostics_remove(view);
    gtk_window_destroy(GTK_WINDOW(window));g_object_unref(context);g_object_unref(session);g_main_loop_unref(loop);
    return 0;
}
