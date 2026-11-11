#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include "webview_session.h"
static GMainLoop *loop;
static const char *expected;
static int result=1;
static gboolean finish(gpointer data) {(void)data;g_main_loop_quit(loop);return G_SOURCE_REMOVE;}
static void received(WebKitUserContentManager *manager,JSCValue *value,gpointer data)
{
    (void)manager;(void)data;char *message=jsc_value_to_string(value);
    if (!g_strcmp0(message,expected)) {result=0;puts("APP_DOM localStorage PASS");}
    else puts("APP_DOM localStorage FAIL");
    g_free(message);g_timeout_add(500,finish,NULL);
}
int main(int argc,char **argv)
{
    if (argc!=4 || !g_str_has_prefix(argv[2],"http://127.0.0.1:") || getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init();loop=g_main_loop_new(NULL,FALSE);expected=argv[3];GError *error=NULL;
    WebKitNetworkSession *session=atl_webview_session_for_app(argv[1],&error);if(!session||error)return 2;
    webkit_network_session_set_proxy_settings(session,WEBKIT_NETWORK_PROXY_MODE_NO_PROXY,NULL);
    WebKitWebContext *context=webkit_web_context_new();
    const char *prefix=getenv("CLBB_WEBKIT_PROCESS_DIR");if(prefix)webkit_web_context_add_path_to_sandbox(context,prefix,TRUE);
    WebKitUserContentManager *manager=webkit_user_content_manager_new();
    g_signal_connect(manager,"script-message-received::storage",G_CALLBACK(received),NULL);
    webkit_user_content_manager_register_script_message_handler(manager,"storage",NULL);
    WebKitWebView *view=WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,"web-context",context,"network-session",session,"user-content-manager",manager,NULL));
    GtkWidget *window=gtk_window_new();gtk_window_set_default_size(GTK_WINDOW(window),500,300);
    gtk_window_set_child(GTK_WINDOW(window),GTK_WIDGET(view));gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_uri(view,argv[2]);g_timeout_add_seconds(12,finish,NULL);g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));g_object_unref(manager);g_object_unref(context);g_main_loop_unref(loop);return result;
}
