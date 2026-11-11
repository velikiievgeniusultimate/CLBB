#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include "webview_cookie_policy.h"

static GMainLoop *loop;
static int result = 1;
static gboolean expected;

static void received(WebKitUserContentManager *manager, JSCValue *value, gpointer data)
{
    (void)manager; (void)data;
    char *message = jsc_value_to_string(value);
    gboolean present = !g_strcmp0(message, "CLBB_COOKIE_1");
    if ((!g_strcmp0(message, "CLBB_COOKIE_0") || present) && present == expected) {
        printf("COOKIE_FLOW policy_allow=%d received_cookie=%d PASS\n", expected, present);
        result = 0;
    } else puts("COOKIE_FLOW mismatch=1");
    g_free(message);
    g_main_loop_quit(loop);
}

static gboolean timeout(gpointer data)
{
    (void)data; puts("COOKIE_FLOW timeout=1"); g_main_loop_quit(loop); return G_SOURCE_REMOVE;
}

int main(int argc, char **argv)
{
    if (argc != 4 || getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init(); loop = g_main_loop_new(NULL, FALSE); expected = !g_strcmp0(argv[3], "allow");
    WebKitWebContext *context = webkit_web_context_new();
    const char *prefix = getenv("CLBB_WEBKIT_PROCESS_DIR");
    if (prefix) webkit_web_context_add_path_to_sandbox(context, prefix, TRUE);
    WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
    GError *error = NULL;
    GTlsCertificate *certificate = g_tls_certificate_new_from_file(argv[2], &error);
    if (!certificate || error) return 2;
    /* Explicit trust only for this local test certificate and these loopback hosts. */
    webkit_network_session_allow_tls_certificate_for_host(session, certificate, "localhost");
    webkit_network_session_allow_tls_certificate_for_host(session, certificate, "127.0.0.1");
    webkit_network_session_set_proxy_settings(session, WEBKIT_NETWORK_PROXY_MODE_NO_PROXY, NULL);
    WebKitUserContentManager *manager = webkit_user_content_manager_new();
    g_signal_connect(manager, "script-message-received::cookie", G_CALLBACK(received), NULL);
    webkit_user_content_manager_register_script_message_handler(manager, "cookie", NULL);
    WebKitWebView *view = WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW, "web-context", context,
        "network-session", session, "user-content-manager", manager, NULL));
    if (atl_webview_cookie_policy_get(view)) return 2;
    if (!atl_webview_cookie_policy_set(view, expected, &error) || error) return 2;
    if (atl_webview_cookie_policy_get(view) != expected) return 2;
    if (!atl_webview_cookie_policy_activate(view, &error) || error) return 2;
    WebKitWebView *other = WEBKIT_WEB_VIEW(g_object_ref_sink(g_object_new(WEBKIT_TYPE_WEB_VIEW,
        "web-context", context, "network-session", session, NULL)));
    if (!atl_webview_cookie_policy_set(other, !expected, &error) || error) return 2;
    if (atl_webview_cookie_policy_activate(other, &error) || !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_NOT_SUPPORTED)) return 2;
    g_clear_error(&error);
    atl_webview_cookie_policy_release(view);
    if (!atl_webview_cookie_policy_activate(other, &error) || error) return 2;
    if (!atl_webview_cookie_policy_set(view, !expected, &error) || error) return 2;
    if (!atl_webview_cookie_policy_activate(view, &error) || error) return 2;
    if (atl_webview_cookie_policy_set(view, expected, &error) || !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_NOT_SUPPORTED)) return 2;
    g_clear_error(&error);
    if (atl_webview_cookie_policy_get(view) != !expected) return 2;
    atl_webview_cookie_policy_release(other);
    if (!atl_webview_cookie_policy_set(view, expected, &error) || error) return 2;
    g_object_unref(other);
    puts("COOKIE_POLICY mixed-view conflict/release/rollback PASS");
    GtkWidget *window = gtk_window_new(); gtk_window_set_default_size(GTK_WINDOW(window), 640, 400);
    gtk_window_set_child(GTK_WINDOW(window), GTK_WIDGET(view)); gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_uri(view, argv[1]);
    g_timeout_add_seconds(15, timeout, NULL); g_main_loop_run(loop);
    atl_webview_cookie_policy_release(view); gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(manager); g_object_unref(context); g_object_unref(session); g_object_unref(certificate);
    g_main_loop_unref(loop); return result;
}
