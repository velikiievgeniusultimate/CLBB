/* SPDX-License-Identifier: GPL-3.0-or-later
 * Exercise both publisher login forms in an ephemeral WebView without entering data.
 */
#include <gtk/gtk.h>
#include <webkit/webkit.h>

static GMainLoop *loop;
static unsigned stage;
static gboolean pending, passed;
static const char *checks[] = {
    "if(typeof LoadPage==='function' && LoadPage.clbbEmailPreference){"
    "LoadPage('login-new','en','clbb-ui-check,'+btoa(JSON.stringify({userToken:'',passwdUrl:'https://mtacc.mobilelegends.com/',encodeParam:''})),'clbb-ui-check');true}",
    "if(location.pathname==='/v3.0/inapp/loginbyemail' && document.querySelectorAll('input').length>=2 && "
    "!document.querySelector('input[type=password]') && document.querySelector('#clbb-login-method button')){"
    "document.querySelector('#clbb-login-method button').click();true}",
    "if(location.pathname==='/v3.0/inapp/login-new' && document.querySelector('input[type=password]') && "
    "document.querySelector('#clbb-login-method button')){document.querySelector('#clbb-login-method button').click();true}",
    "location.pathname==='/v3.0/inapp/loginbyemail' && document.querySelectorAll('input').length>=2 && "
    "!document.querySelector('input[type=password]') && !!document.querySelector('#clbb-login-method button')",
};

static void checked(GObject *object, GAsyncResult *result, gpointer unused) {
    (void)unused;
    GError *error = NULL;
    JSCValue *value = webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(object),result,&error);
    if (value && jsc_value_is_boolean(value) && jsc_value_to_boolean(value)) {
        if (++stage == G_N_ELEMENTS(checks)) {
            passed = TRUE;
            g_print("MOONTON_FORMS_OK SDK to code, code to password, password to code; no submission\n");
            g_main_loop_quit(loop);
        }
    }
    g_clear_object(&value);
    g_clear_error(&error);
    pending = FALSE;
}

static gboolean inspect(gpointer view) {
    if (!pending) {
        pending = TRUE;
        webkit_web_view_evaluate_javascript(view,checks[stage],-1,NULL,NULL,NULL,checked,NULL);
    }
    return G_SOURCE_CONTINUE;
}

static gboolean expired(gpointer unused) {
    (void)unused;
    g_printerr("Login form check timed out at step %u\n",stage+1);
    g_main_loop_quit(loop);
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    gtk_init();
    loop = g_main_loop_new(NULL,FALSE);
    WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
    WebKitWebView *view = WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,"network-session",session,NULL));
    char *source = NULL;
    if (!g_file_get_contents(argv[1],&source,NULL,NULL)) return 2;
    const char *allowed[] = {"https://mtacc.mobilelegends.com/*",NULL};
    WebKitUserScript *policy = webkit_user_script_new(source,WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
        WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_END,allowed,NULL);
    webkit_user_content_manager_add_script(webkit_web_view_get_user_content_manager(view),policy);
    webkit_user_script_unref(policy);
    g_free(source);
    GtkWidget *window = gtk_window_new();
    gtk_window_set_default_size(GTK_WINDOW(window),900,650);
    gtk_window_set_child(GTK_WINDOW(window),GTK_WIDGET(view));
    gtk_window_present(GTK_WINDOW(window));
    webkit_web_view_load_uri(view,"https://mtacc.mobilelegends.com/");
    g_timeout_add(250,inspect,view);
    g_timeout_add_seconds(45,expired,NULL);
    g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(session);
    g_main_loop_unref(loop);
    return passed ? 0 : 1;
}
