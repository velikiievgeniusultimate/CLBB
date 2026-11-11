/* SPDX-License-Identifier: GPL-3.0-or-later
 * Regression fixture: exercise synchronous prompt ownership in real WebKitGTK.
 * --old-close reproduces the previous premature-close failure (run with core dumps disabled).
 */
#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <string.h>
static GMainLoop *loop;
static gboolean old_close, failed = TRUE;
static unsigned prompts;
static gboolean dialog_cb(WebKitWebView *view, WebKitScriptDialog *dialog, void *unused)
{
    if (webkit_script_dialog_get_dialog_type(dialog) != WEBKIT_SCRIPT_DIALOG_PROMPT)
        return FALSE;
    webkit_script_dialog_ref(dialog);
    g_object_ref(view);
    webkit_script_dialog_prompt_set_text(dialog, "fixture-response");
    if (old_close) webkit_script_dialog_close(dialog);
    webkit_script_dialog_unref(dialog);
    g_object_unref(view);
    ++prompts;
    return TRUE;
}
static void finished(GObject *view, GAsyncResult *result, void *unused)
{
    GError *error = NULL;
    JSCValue *value = webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(view), result, &error);
    failed = !value || !jsc_value_is_number(value) || jsc_value_to_int32(value) != 128 || prompts != 128;
    if (error) { g_printerr("%s\n", error->message); g_error_free(error); }
    if (value) g_object_unref(value);
    g_main_loop_quit(loop);
}
static void loaded(WebKitWebView *view, WebKitLoadEvent event, void *unused)
{
    if (event != WEBKIT_LOAD_FINISHED) return;
    const char *script = "(()=>{let n=0;for(let i=0;i<128;i++){if(prompt('__ATL_JAVASCRIPT__fixture','')==='fixture-response')n++;}return n;})()";
    webkit_web_view_evaluate_javascript(view, script, -1, NULL, NULL, NULL, finished, NULL);
}
static gboolean expired(void *unused) { g_printerr("Fixture timeout\n"); g_main_loop_quit(loop); return G_SOURCE_REMOVE; }
int main(int argc, char **argv)
{
    old_close = argc > 1 && !strcmp(argv[1], "--old-close");
    gtk_init(); loop = g_main_loop_new(NULL, FALSE);
    GtkWidget *window = gtk_window_new(), *view = webkit_web_view_new();
    gtk_window_set_child(GTK_WINDOW(window), view);
    g_signal_connect(view, "script-dialog", G_CALLBACK(dialog_cb), NULL);
    g_signal_connect(view, "load-changed", G_CALLBACK(loaded), NULL);
    gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_html(WEBKIT_WEB_VIEW(view), "<html><body>Owned regression fixture</body></html>", "https://fixture.invalid/");
    g_timeout_add_seconds(15, expired, NULL); g_main_loop_run(loop);
    if (!failed) g_print("WEBKIT_PROMPTS_OK count=%u\n", prompts);
    gtk_window_destroy(GTK_WINDOW(window)); g_main_loop_unref(loop);
    return failed ? 1 : 0;
}
