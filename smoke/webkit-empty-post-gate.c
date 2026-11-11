#include <webkit/webkit-web-process-extension.h>
#include <string.h>

static gboolean send_request(WebKitWebPage *page, WebKitURIRequest *request,
                             WebKitURIResponse *response, gpointer data)
{
    (void)page; (void)response; (void)data;
    const char *method = webkit_uri_request_get_http_method(request);
    if (!method) return TRUE;
    GUri *uri = g_uri_parse(webkit_uri_request_get_uri(request), G_URI_FLAGS_NONE, NULL);
    if (!uri) return TRUE;
    const char *host = g_uri_get_host(uri);
    gboolean allowed = !g_strcmp0(g_uri_get_scheme(uri), "https") &&
        !g_strcmp0(g_uri_get_path(uri), "/") && !g_uri_get_query(uri) && !g_uri_get_userinfo(uri) &&
        (g_uri_get_port(uri) == -1 || g_uri_get_port(uri) == 443) && host &&
        (!strcmp(host, "mtacc.mobilelegends.com") ||
         !strcmp(host, "accountmtapi.mobilelegends.com"));
    if (allowed && !strcmp(method, "POST"))
        allowed = !strcmp(host, "accountmtapi.mobilelegends.com");
    else if (allowed && strcmp(method, "GET") && strcmp(method, "HEAD") && strcmp(method, "OPTIONS"))
        allowed = FALSE;
    g_uri_unref(uri);
    return !allowed;
}

static void page_created(WebKitWebProcessExtension *extension, WebKitWebPage *page, gpointer data)
{
    (void)extension; (void)data;
    g_signal_connect(page, "send-request", G_CALLBACK(send_request), NULL);
    webkit_web_page_send_message_to_view(page, webkit_user_message_new("root-only-gate", NULL), NULL, NULL, NULL);
}

G_MODULE_EXPORT void webkit_web_process_extension_initialize(WebKitWebProcessExtension *extension)
{
    g_signal_connect(extension, "page-created", G_CALLBACK(page_created), NULL);
}
