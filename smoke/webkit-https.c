#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <stdio.h>

struct row { const char *host; WebKitWebView *view; gboolean done, gate, cors_fail; guint status; char *domain; int code; };
static struct row rows[] = {
    { .host = "mtacc.mobilelegends.com" },
    { .host = "accountmtapi.mobilelegends.com" },
    { .host = "mtpassapi.mobilelegends.com" }
};
static GMainLoop *loop;
static gboolean fetching, finished;
static int outcome = 1;

static void report(void)
{
    if (finished) return;
    finished = TRUE;
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i) {
        struct row *r = &rows[i];
        printf("hostname=%s status=%u gerror_domain=%s gerror_code=%d cors_fail=%s\n",
               r->host, r->status, r->domain ? r->domain : "none", r->code,
               r->cors_fail ? "true" : "false");
    }
    fflush(stdout);
    g_main_loop_quit(loop);
}

static void store_error(struct row *row, GError *error)
{
    g_free(row->domain);
    row->domain = g_strdup(g_quark_to_string(error->domain));
    row->code = error->code;
}

static void fetched(GObject *object, GAsyncResult *result, gpointer data)
{
    (void)data;
    GError *error = NULL;
    JSCValue *value = webkit_web_view_call_async_javascript_function_finish(WEBKIT_WEB_VIEW(object), result, &error);
    if (!value) {
        if (error) { store_error(&rows[0], error); g_clear_error(&error); }
        rows[1].cors_fail = rows[2].cors_fail = TRUE;
        report();
        return;
    }
    gboolean valid = jsc_value_is_array(value);
    for (unsigned i = 0; i < 2 && valid; ++i) {
        JSCValue *entry = jsc_value_object_get_property_at_index(value, i);
        JSCValue *host_value = jsc_value_object_get_property(entry, "host");
        JSCValue *status_value = jsc_value_object_get_property(entry, "status");
        JSCValue *cors_value = jsc_value_object_get_property(entry, "cors_fail");
        char *host = jsc_value_to_string(host_value);
        valid = !g_strcmp0(host, rows[i + 1].host);
        rows[i + 1].cors_fail = jsc_value_to_boolean(cors_value);
        /* Native GET supplies status even if the CORS fetch cannot expose it. */
        if (!rows[i + 1].cors_fail) rows[i + 1].status = (guint)jsc_value_to_int32(status_value);
        g_free(host);
        g_object_unref(cors_value); g_object_unref(status_value); g_object_unref(host_value); g_object_unref(entry);
    }
    g_object_unref(value);
    if (valid) outcome = 0;
    report();
}

static void maybe_fetch(void)
{
    if (fetching || finished) return;
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i)
        if (!rows[i].done || !rows[i].gate) return;
    if (rows[0].domain) { report(); return; }
    fetching = TRUE;
    /* Page scripts were disabled throughout loading. Only this trusted test body now executes. */
    webkit_settings_set_enable_javascript(webkit_web_view_get_settings(rows[0].view), TRUE);
    const char *body =
        "return await Promise.all(['accountmtapi.mobilelegends.com','mtpassapi.mobilelegends.com'].map(async host=>{"
        "const controller=new AbortController(); const timer=setTimeout(()=>controller.abort(),8000);"
        "try { const response=await fetch('https://'+host+'/',{method:'GET',mode:'cors',credentials:'omit',"
        "cache:'no-store',redirect:'error',signal:controller.signal});"
        "return {host,status:response.status,cors_fail:false};"
        "} catch (_) { return {host,status:0,cors_fail:true}; } finally {clearTimeout(timer);} }));";
    webkit_web_view_call_async_javascript_function(rows[0].view, body, -1, NULL, NULL, NULL, NULL, fetched, NULL);
}

static gboolean user_message(WebKitWebView *view, WebKitUserMessage *message, gpointer data)
{
    (void)view;
    struct row *row = data;
    if (g_strcmp0(webkit_user_message_get_name(message), "root-only-gate")) return FALSE;
    row->gate = TRUE;
    maybe_fetch();
    return TRUE;
}

static void loaded(WebKitWebView *view, WebKitLoadEvent event, gpointer data)
{
    struct row *row = data;
    if (event != WEBKIT_LOAD_FINISHED || row->done) return;
    WebKitWebResource *resource = webkit_web_view_get_main_resource(view);
    WebKitURIResponse *response = resource ? webkit_web_resource_get_response(resource) : NULL;
    if (response) row->status = webkit_uri_response_get_status_code(response);
    row->done = TRUE;
    maybe_fetch();
}

static gboolean failed(WebKitWebView *view, WebKitLoadEvent event, const char *uri, GError *error, gpointer data)
{
    (void)view; (void)event; (void)uri;
    struct row *row = data;
    if (!row->done) { store_error(row, error); row->done = TRUE; }
    maybe_fetch();
    return TRUE;
}

static gboolean authenticate(WebKitWebView *view, WebKitAuthenticationRequest *request, gpointer data)
{
    (void)view; (void)data;
    webkit_authentication_request_cancel(request);
    return TRUE;
}

static gboolean deadline(gpointer data)
{
    (void)data;
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i) {
        if (!rows[i].done || !rows[i].gate) {
            GError *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_TIMED_OUT, "bounded smoke deadline");
            store_error(&rows[i], error);
            g_error_free(error);
        }
    }
    report();
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv)
{
    if (argc != 3 || getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init();
    loop = g_main_loop_new(NULL, FALSE);
    WebKitWebContext *context = webkit_web_context_new();
    webkit_web_context_add_path_to_sandbox(context, argv[1], TRUE);
    webkit_web_context_add_path_to_sandbox(context, argv[2], TRUE);
    webkit_web_context_set_web_process_extensions_directory(context, argv[2]);
    WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
    webkit_network_session_set_tls_errors_policy(session, WEBKIT_TLS_ERRORS_POLICY_FAIL);
    webkit_cookie_manager_set_accept_policy(webkit_network_session_get_cookie_manager(session), WEBKIT_COOKIE_POLICY_ACCEPT_NEVER);
    GtkWidget *window = gtk_window_new(), *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_window_set_child(GTK_WINDOW(window), box);
    gtk_window_set_default_size(GTK_WINDOW(window), 640, 480);
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i) {
        struct row *row = &rows[i];
        row->view = WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW, "web-context", context, "network-session", session, NULL));
        webkit_settings_set_enable_javascript(webkit_web_view_get_settings(row->view), FALSE);
        g_signal_connect(row->view, "load-changed", G_CALLBACK(loaded), row);
        g_signal_connect(row->view, "load-failed", G_CALLBACK(failed), row);
        g_signal_connect(row->view, "user-message-received", G_CALLBACK(user_message), row);
        g_signal_connect(row->view, "authenticate", G_CALLBACK(authenticate), NULL);
        gtk_widget_set_vexpand(GTK_WIDGET(row->view), TRUE);
        gtk_box_append(GTK_BOX(box), GTK_WIDGET(row->view));
    }
    gtk_window_present(GTK_WINDOW(window));
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i) {
        char *uri = g_strdup_printf("https://%s/", rows[i].host);
        webkit_web_view_load_uri(rows[i].view, uri);
        g_free(uri);
    }
    g_timeout_add_seconds(25, deadline, NULL);
    g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(session); g_object_unref(context); g_main_loop_unref(loop);
    for (unsigned i = 0; i < G_N_ELEMENTS(rows); ++i) g_free(rows[i].domain);
    return outcome;
}
