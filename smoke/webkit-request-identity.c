#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <stdio.h>
static GMainLoop *loop;
static int outcome = 1;
static void message(WebKitUserContentManager *manager, JSCValue *value, gpointer data)
{
    (void)manager; (void)data;
    if (jsc_value_is_boolean(value) && jsc_value_to_boolean(value)) {
        puts("REQUEST_IDENTITY PASS main=1 subresource=1 fetch-default=1 fetch-override=1 xhr-override=1 empty-override=1");
        outcome = 0;
    } else puts("REQUEST_IDENTITY FAIL assertions");
    g_main_loop_quit(loop);
}
static gboolean deadline(gpointer data)
{
    (void)data;
    puts("REQUEST_IDENTITY FAIL timeout");
    g_main_loop_quit(loop);
    return G_SOURCE_REMOVE;
}
int main(int argc, char **argv)
{
    if (argc != 5 || !g_str_has_prefix(argv[3], "http://127.0.0.1:") ||
        getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init();
    loop = g_main_loop_new(NULL, FALSE);
    WebKitWebContext *context = webkit_web_context_new();
    webkit_web_context_add_path_to_sandbox(context, argv[1], TRUE);
    webkit_web_context_add_path_to_sandbox(context, argv[2], TRUE);
    webkit_web_context_set_web_process_extensions_directory(context, argv[2]);
    GVariant *data = !g_strcmp0(argv[4], "invalid-type") ? g_variant_new_boolean(TRUE) : g_variant_new_string(argv[4]);
    webkit_web_context_set_web_process_extensions_initialization_user_data(context, data);
    WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
    webkit_network_session_set_proxy_settings(session, WEBKIT_NETWORK_PROXY_MODE_NO_PROXY, NULL);
    webkit_network_session_set_tls_errors_policy(session, WEBKIT_TLS_ERRORS_POLICY_FAIL);
    WebKitUserContentManager *manager = webkit_user_content_manager_new();
    g_signal_connect(manager, "script-message-received::test", G_CALLBACK(message), NULL);
    if (!webkit_user_content_manager_register_script_message_handler(manager, "test", NULL)) return 2;
    GtkWidget *view = g_object_new(WEBKIT_TYPE_WEB_VIEW, "web-context", context,
        "network-session", session, "user-content-manager", manager, NULL);
    GtkWidget *window = gtk_window_new();
    gtk_window_set_default_size(GTK_WINDOW(window), 640, 480);
    gtk_window_set_child(GTK_WINDOW(window), view);
    gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_uri(WEBKIT_WEB_VIEW(view), argv[3]);
    g_timeout_add_seconds(15, deadline, NULL);
    g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(manager); g_object_unref(session); g_object_unref(context); g_main_loop_unref(loop);
    return outcome;
}
