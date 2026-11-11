#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <glib/gstdio.h>
#include "webview_session.h"

static GMainLoop *loop;
static WebKitNetworkSession *first, *second;
static gboolean writing, checking_second;
static int result = 1;
static const char *uri = "https://clbb-storage.invalid/";

static gboolean done(gpointer data)
{
    (void)data; g_main_loop_quit(loop); return G_SOURCE_REMOVE;
}

static void cookies(GObject *object, GAsyncResult *async, gpointer data)
{
    (void)data; GError *error = NULL;
    GList *list = webkit_cookie_manager_get_cookies_finish(WEBKIT_COOKIE_MANAGER(object), async, &error);
    gboolean present = FALSE;
    for (GList *item = list; item; item = item->next) {
        SoupCookie *cookie = item->data;
        if (!g_strcmp0(soup_cookie_get_name(cookie), "clbb_storage_fixture") &&
            !g_strcmp0(soup_cookie_get_value(cookie), "local_test_only")) present = TRUE;
    }
    g_list_free_full(list, (GDestroyNotify)soup_cookie_free);
    if (error || present == checking_second) {
        puts("APP_STORAGE cookie_isolation_or_persistence FAIL");
        g_clear_error(&error); g_main_loop_quit(loop); return;
    }
    if (!checking_second) {
        checking_second = TRUE;
        webkit_cookie_manager_get_cookies(webkit_network_session_get_cookie_manager(second), uri, NULL, cookies, NULL);
    } else {
        printf("APP_STORAGE mode=%s own_cookie=1 other_app_cookie=0 PASS\n", writing ? "write" : "restart-read");
        result = 0;
        /* Let the normal network process finish its pending persistent-store I/O. */
        if (writing) g_timeout_add(1000, done, NULL); else g_main_loop_quit(loop);
    }
}

static void added(GObject *object, GAsyncResult *async, gpointer data)
{
    (void)data; GError *error = NULL;
    if (!webkit_cookie_manager_add_cookie_finish(WEBKIT_COOKIE_MANAGER(object), async, &error)) {
        g_clear_error(&error); g_main_loop_quit(loop); return;
    }
    webkit_cookie_manager_get_cookies(webkit_network_session_get_cookie_manager(first), uri, NULL, cookies, NULL);
}

int main(int argc, char **argv)
{
    if (argc != 4 || getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) return 2;
    gtk_init(); loop = g_main_loop_new(NULL, FALSE); writing = !g_strcmp0(argv[3], "write");
    GError *error = NULL;
    if (atl_webview_session_for_app("relative", &error) || !g_error_matches(error, G_IO_ERROR, G_IO_ERROR_INVALID_ARGUMENT)) return 2;
    g_clear_error(&error);
    first = atl_webview_session_for_app(argv[1], &error);
    if (!first || error || first != atl_webview_session_for_app(argv[1], &error)) return 2;
    second = atl_webview_session_for_app(argv[2], &error);
    if (!second || error || second == first) return 2;
    char *base = g_build_filename(argv[1], "app_webview", NULL);
    GStatBuf st;
    if (g_stat(base, &st) || (st.st_mode & 0777) != 0700) return 2;
    g_free(base);
    puts("APP_STORAGE app_scope/shared_session/private_permissions PASS");
    if (writing) {
        GUri *parsed = g_uri_parse(uri, G_URI_FLAGS_NONE, NULL);
        SoupCookie *cookie = soup_cookie_parse("clbb_storage_fixture=local_test_only; Max-Age=3600; Path=/; Secure; HttpOnly", parsed);
        if (!cookie) return 2;
        webkit_cookie_manager_add_cookie(webkit_network_session_get_cookie_manager(first), cookie, NULL, added, NULL);
        soup_cookie_free(cookie); g_uri_unref(parsed);
    } else webkit_cookie_manager_get_cookies(webkit_network_session_get_cookie_manager(first), uri, NULL, cookies, NULL);
    g_timeout_add_seconds(12, done, NULL); g_main_loop_run(loop); g_main_loop_unref(loop); return result;
}
