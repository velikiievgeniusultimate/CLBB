/* SPDX-License-Identifier: GPL-3.0-or-later
 * Read-only real-engine check of the publisher's login UI. Never fills/submits inputs.
 */
#include <gtk/gtk.h>
#include <webkit/webkit.h>
static GMainLoop *loop; static gboolean routed, passed;
static void checked(GObject *object, GAsyncResult *result, void *unused) {
 GError *error=NULL; JSCValue *value=webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(object),result,&error);
 if(value&&jsc_value_is_boolean(value)&&jsc_value_to_boolean(value)){passed=TRUE;g_print("MOONTON_EMAIL_FORM_OK code_inputs_present password_inputs_absent no_submission\n");g_main_loop_quit(loop);}
 if(value)g_object_unref(value);g_clear_error(&error);
}
static gboolean inspect(void *data) {
 WebKitWebView *view=data;
 const char *script="location.pathname==='/v3.0/inapp/loginbyemail'&&document.querySelectorAll('input[type=password]').length===0&&document.querySelectorAll('input').length>=2";
 webkit_web_view_evaluate_javascript(view,script,-1,NULL,NULL,NULL,checked,NULL);return G_SOURCE_CONTINUE;
}
static void loaded(WebKitWebView *view, WebKitLoadEvent event, void *unused) {
 if(event!=WEBKIT_LOAD_FINISHED||routed)return;routed=TRUE;
 const char *script="if(typeof LoadPage==='function')LoadPage('login-new','en','clbb-ui-check,'+btoa(JSON.stringify({userToken:'',passwdUrl:'https://mtacc.mobilelegends.com/',encodeParam:''})),'clbb-ui-check')";
 webkit_web_view_evaluate_javascript(view,script,-1,NULL,NULL,NULL,NULL,NULL);
}
static gboolean expired(void *unused){g_printerr("EMAIL_FORM_TIMEOUT\n");g_main_loop_quit(loop);return G_SOURCE_REMOVE;}
int main(int argc,char **argv) {
 if(argc!=2)return 2;gtk_init();loop=g_main_loop_new(NULL,FALSE);
 WebKitNetworkSession *session=webkit_network_session_new_ephemeral();
 WebKitWebView *view=WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,"network-session",session,NULL));
 char *source=NULL;if(!g_file_get_contents(argv[1],&source,NULL,NULL))return 2;
 const char *allowed[]={"https://mtacc.mobilelegends.com/*",NULL};
 WebKitUserScript *policy=webkit_user_script_new(source,WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_END,allowed,NULL);
 webkit_user_content_manager_add_script(webkit_web_view_get_user_content_manager(view),policy);webkit_user_script_unref(policy);g_free(source);
 g_signal_connect(view,"load-changed",G_CALLBACK(loaded),NULL);
 GtkWidget *window=gtk_window_new();gtk_window_set_default_size(GTK_WINDOW(window),900,650);gtk_window_set_child(GTK_WINDOW(window),GTK_WIDGET(view));gtk_window_present(GTK_WINDOW(window));
 webkit_web_view_load_uri(view,"https://mtacc.mobilelegends.com/");
 g_timeout_add(300,inspect,view);g_timeout_add_seconds(45,expired,NULL);g_main_loop_run(loop);
 gtk_window_destroy(GTK_WINDOW(window));g_object_unref(session);g_main_loop_unref(loop);return passed?0:1;
}
