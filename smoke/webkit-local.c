#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static GMainLoop *loop;
static WebKitWebView *view;
static const char *snapshot_path;
static int outcome = 1;
static gboolean loaded, prompted, messaged, evaluating;
static gboolean saw_web, saw_network, saw_sandbox;
static const char *input_text = "Привет, 世界 👋";
static const char *native_reply = "native-ответ-✓";

static void fail(const char *stage, const char *detail)
{
    fprintf(stderr, "WEBKIT_SMOKE_FAIL stage=%s detail=%s\n", stage, detail ? detail : "unknown");
    g_main_loop_quit(loop);
}

/* Inspect only this smoke process's descendants, never unrelated applications. */
static void inspect_helpers(pid_t parent, unsigned depth)
{
    if (depth > 8) return;
    char *path = g_strdup_printf("/proc/%ld/task/%ld/children", (long)parent, (long)parent);
    char *children = NULL;
    g_file_get_contents(path, &children, NULL, NULL);
    g_free(path);
    if (!children) return;
    char **ids = g_strsplit(children, " ", -1);
    for (unsigned i = 0; ids[i]; ++i) {
        pid_t child = (pid_t)g_ascii_strtoll(ids[i], NULL, 10);
        if (child <= 0) continue;
        path = g_strdup_printf("/proc/%ld/comm", (long)child);
        char *name = NULL;
        g_file_get_contents(path, &name, NULL, NULL);
        g_free(path);
        if (name && g_str_has_prefix(name, "WebKit")) {
            g_strchomp(name);
            path = g_strdup_printf("/proc/%ld/status", (long)child);
            char *status = NULL;
            g_file_get_contents(path, &status, NULL, NULL);
            g_free(path);
            int nnp = -1, seccomp = -1;
            if (status) {
                const char *field = strstr(status, "NoNewPrivs:");
                if (field) sscanf(field, "NoNewPrivs: %d", &nnp);
                field = strstr(status, "Seccomp:");
                if (field) sscanf(field, "Seccomp: %d", &seccomp);
            }
            fprintf(stderr, "WEBKIT_SMOKE_HELPER pid=%ld name=%s no-new-privs=%d seccomp=%d\n",
                    (long)child, name, nnp, seccomp);
            if (g_str_has_prefix(name, "WebKitWebProc")) {
                saw_web = TRUE;
                if (nnp == 1 && seccomp == 2) saw_sandbox = TRUE;
            }
            if (g_str_has_prefix(name, "WebKitNetwork")) saw_network = TRUE;
            g_free(status);
        }
        g_free(name);
        inspect_helpers(child, depth + 1);
    }
    g_strfreev(ids);
    g_free(children);
}

static void snapshot_finished(GObject *object, GAsyncResult *result, gpointer data)
{
    (void)data;
    GError *error = NULL;
    GdkTexture *texture = webkit_web_view_get_snapshot_finish(WEBKIT_WEB_VIEW(object), result, &error);
    if (!texture) {
        fail("snapshot", error ? error->message : "no texture");
        g_clear_error(&error);
        return;
    }
    int width = gdk_texture_get_width(texture), height = gdk_texture_get_height(texture);
    gsize stride = (gsize)width * 4;
    guchar *pixels = g_malloc(stride * height);
    gdk_texture_download(texture, pixels, stride);
    gboolean varied = FALSE;
    for (gsize offset = 4; offset < stride * height; offset += 4)
        if (memcmp(pixels, pixels + offset, 3)) { varied = TRUE; break; }
    g_free(pixels);
    gboolean saved = gdk_texture_save_to_png(texture, snapshot_path);
    fprintf(stderr, "WEBKIT_SMOKE_RENDER width=%d height=%d varied=%d saved=%d\n", width, height, varied, saved);
    g_object_unref(texture);
    inspect_helpers(getpid(), 0);
    if (!varied || !saved || width < 100 || height < 100) {
        fail("render", "empty, uniform, or unsaved snapshot");
        return;
    }
    if (!saw_web || !saw_network || !saw_sandbox) {
        fail("helpers", "expected WebKitWebProcess/NetworkProcess with sandbox no-new-privs=1 seccomp=2");
        return;
    }
    puts("WEBKIT_SMOKE_OK load=1 unicode-input=1 prompt=1 script-message=1 dom=1 render=1 sandbox=1");
    outcome = 0;
    g_main_loop_quit(loop);
}

static void evaluated(GObject *object, GAsyncResult *result, gpointer data)
{
    (void)data;
    GError *error = NULL;
    JSCValue *value = webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(object), result, &error);
    if (!value) {
        fail("evaluate-javascript", error ? error->message : "no result");
        g_clear_error(&error);
        return;
    }
    char *actual = jsc_value_to_string(value);
    char *expected = g_strdup_printf("%s|%s", input_text, native_reply);
    gboolean correct = !g_strcmp0(actual, expected);
    g_free(actual);
    g_free(expected);
    g_object_unref(value);
    if (!correct) { fail("dom", "Unicode input/native reply mismatch"); return; }
    webkit_web_view_get_snapshot(view, WEBKIT_SNAPSHOT_REGION_VISIBLE, WEBKIT_SNAPSHOT_OPTIONS_NONE,
                                NULL, snapshot_finished, NULL);
}

static void maybe_evaluate(void)
{
    if (!loaded || !prompted || !messaged || evaluating) return;
    evaluating = TRUE;
    webkit_web_view_evaluate_javascript(view,
        "document.querySelector('#field').value+'|'+document.querySelector('#result').textContent",
        -1, NULL, NULL, NULL, evaluated, NULL);
}

static void script_message(WebKitUserContentManager *manager, JSCValue *value, gpointer data)
{
    (void)manager; (void)data;
    char *actual = jsc_value_to_string(value);
    char *expected = g_strdup_printf("%s|%s|complete", input_text, native_reply);
    gboolean correct = !g_strcmp0(actual, expected);
    g_free(actual); g_free(expected);
    if (!correct) { fail("script-message", "unexpected test result"); return; }
    messaged = TRUE;
    fprintf(stderr, "WEBKIT_SMOKE_MESSAGE unicode/native-reply/document-state verified\n");
    maybe_evaluate();
}

static gboolean script_dialog(WebKitWebView *webview, WebKitScriptDialog *dialog, gpointer data)
{
    (void)webview; (void)data;
    if (webkit_script_dialog_get_dialog_type(dialog) != WEBKIT_SCRIPT_DIALOG_PROMPT ||
        g_strcmp0(webkit_script_dialog_get_message(dialog), "CLBB_NATIVE_PROMPT") ||
        g_strcmp0(webkit_script_dialog_prompt_get_default_text(dialog), "запрос")) {
        fail("prompt", "unexpected dialog");
        return TRUE;
    }
    prompted = TRUE;
    webkit_script_dialog_prompt_set_text(dialog, native_reply);
    fprintf(stderr, "WEBKIT_SMOKE_PROMPT native synchronous reply supplied\n");
    return TRUE;
}

static void load_changed(WebKitWebView *webview, WebKitLoadEvent event, gpointer data)
{
    (void)webview; (void)data;
    fprintf(stderr, "WEBKIT_SMOKE_LOAD state=%d\n", event);
    if (event == WEBKIT_LOAD_FINISHED) { loaded = TRUE; maybe_evaluate(); }
}

static gboolean load_failed(WebKitWebView *webview, WebKitLoadEvent event, const char *uri, GError *error, gpointer data)
{
    (void)webview; (void)event; (void)uri; (void)data;
    fail("load", error->message);
    return TRUE;
}

static void terminated(WebKitWebView *webview, WebKitWebProcessTerminationReason reason, gpointer data)
{
    (void)webview; (void)data;
    char *detail = g_strdup_printf("reason=%d", reason);
    fail("web-process-terminated", detail);
    g_free(detail);
}

static gboolean deadline(gpointer data)
{
    (void)data;
    fail("deadline", "25-second in-process deadline");
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv)
{
    if (argc != 3 || !g_str_has_prefix(argv[1], "http://127.0.0.1:")) {
        fprintf(stderr, "Usage: webkit-local http://127.0.0.1:PORT/ SNAPSHOT.png\n");
        return 2;
    }
    if (getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS")) {
        fprintf(stderr, "WEBKIT_SMOKE_FAIL stage=configuration detail=sandbox-disable variable is present\n");
        return 2;
    }
    gtk_init();
    loop = g_main_loop_new(NULL, FALSE);
    snapshot_path = argv[2];
    WebKitWebContext *context = webkit_web_context_new();
    /* Relocated helper/shared-library files are read-only in the normal sandbox. */
    const char *prefix = getenv("CLBB_WEBKIT_SMOKE_PREFIX");
    if (prefix) webkit_web_context_add_path_to_sandbox(context, prefix, TRUE);
    WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
    WebKitUserContentManager *manager = webkit_user_content_manager_new();
    g_signal_connect(manager, "script-message-received::smoke", G_CALLBACK(script_message), NULL);
    if (!webkit_user_content_manager_register_script_message_handler(manager, "smoke", NULL)) return 2;
    view = WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW, "web-context", context,
        "network-session", session, "user-content-manager", manager, NULL));
    g_signal_connect(view, "script-dialog", G_CALLBACK(script_dialog), NULL);
    g_signal_connect(view, "load-changed", G_CALLBACK(load_changed), NULL);
    g_signal_connect(view, "load-failed", G_CALLBACK(load_failed), NULL);
    g_signal_connect(view, "web-process-terminated", G_CALLBACK(terminated), NULL);
    GtkWidget *window = gtk_window_new();
    gtk_window_set_default_size(GTK_WINDOW(window), 800, 500);
    gtk_window_set_title(GTK_WINDOW(window), "CLBB isolated local WebKit smoke");
    gtk_window_set_child(GTK_WINDOW(window), GTK_WIDGET(view));
    gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_uri(view, argv[1]);
    g_timeout_add_seconds(25, deadline, NULL);
    g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(manager); g_object_unref(session); g_object_unref(context);
    g_main_loop_unref(loop);
    return outcome;
}
