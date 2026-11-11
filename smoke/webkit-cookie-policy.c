#include <gtk/gtk.h>
#include <webkit/webkit.h>
static GMainLoop *loop;
static int result = 1;
static void ready(GObject *object, GAsyncResult *async, gpointer data)
{
    (void)data;
    GError *error = NULL;
    WebKitCookieAcceptPolicy policy = webkit_cookie_manager_get_accept_policy_finish(
        WEBKIT_COOKIE_MANAGER(object), async, &error);
    if (!error) {
        printf("COOKIE_DEFAULT policy=%d itp=%d\n", policy,
               webkit_network_session_get_itp_enabled(webkit_network_session_get_default()));
        result = 0;
    } else {
        puts("COOKIE_DEFAULT error=1");
        g_error_free(error);
    }
    g_main_loop_quit(loop);
}
int main(void)
{
    gtk_init();
    loop = g_main_loop_new(NULL, FALSE);
    webkit_cookie_manager_get_accept_policy(
        webkit_network_session_get_cookie_manager(webkit_network_session_get_default()), NULL, ready, NULL);
    g_main_loop_run(loop);
    g_main_loop_unref(loop);
    return result;
}
