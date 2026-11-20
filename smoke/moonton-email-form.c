/* SPDX-License-Identifier: GPL-3.0-or-later
 * Exercise the code-only policy in WebKit. All test input is synthetic and stays local.
 */
#include <gtk/gtk.h>
#include <webkit/webkit.h>

static GMainLoop *loop;
static unsigned stage;
static gboolean pending, passed;
static gboolean fixture;
static const char *live_checks[] = {
    "if(typeof LoadPage==='function' && LoadPage.clbbEmailPreference){"
    "LoadPage('login-new','en','clbb-ui-check,'+btoa(JSON.stringify({userToken:'',passwdUrl:'https://mtacc.mobilelegends.com/',encodeParam:''})),'clbb-ui-check');true}",
    "if(location.pathname==='/v3.0/inapp/loginbyemail' && document.querySelectorAll('input').length>=2 && !document.querySelector('input[type=password]')){"
    "let b=document.querySelector('#clbb-password-info') || [...document.querySelectorAll('span,p,a,div')].find(e=>e.childElementCount===0&&/^(log in via account password|use a password)$/i.test(e.textContent.trim()));"
    "if(b){document.querySelector('input').value='fixture@example.invalid'; b.click(); true}}",
    "if(document.querySelector('#clbb-code-dialog[open] h2')?.textContent.includes('Password sign-in is unavailable') && document.querySelector('input').value==='fixture@example.invalid'){document.querySelector('#clbb-code-dialog button').click();true}",
    "location.pathname==='/v3.0/inapp/loginbyemail' && !document.querySelector('#clbb-code-dialog') && !document.querySelector('input[type=password]') && !!document.querySelector('.send-but')",
};
static const char *fixture_html =
    "<html><body><main class='inapp ru'><input id='email' value='fixture@example.invalid'><input id='code'>"
    "<div class='send-but' onclick='sent++'>Отправить код</div><div class='botm-but login-but'>Вход</div>"
    "<div id='original' onclick='switched++'><span>Войти с паролем</span><i id='arrow' tabindex='0'></i></div>"
    "<button id='recover' onclick='recovered++'>Забыли пароль?</button></main>"
    "<script>var sent=0,switched=0,recovered=0</script></body></html>";
static const char *fixture_checks[] = {
    "if(document.querySelector('#clbb-code-style') && !document.querySelector('#clbb-password-info')){document.querySelector('#arrow').click();true}",
    "if(document.querySelector('#clbb-code-dialog[open] h2')?.textContent==='Вход по паролю в CLBB невозможен' && switched===0 && document.querySelector('#email').value==='fixture@example.invalid'){document.querySelector('dialog button').click();true}",
    "if(!document.querySelector('dialog')){document.querySelector('#original span').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));true}",
    "if(document.querySelector('dialog[open]') && !switched){document.querySelector('dialog').close();document.querySelector('.send-but').click();document.querySelector('#recover').click();document.querySelector('#original').remove();true}",
    "if(sent===1 && recovered===1 && document.querySelector('#clbb-password-info')?.textContent==='Войти с паролем'){document.querySelector('#clbb-password-info').click();true}",
    "if(document.querySelector('dialog[open]') && !switched && document.querySelector('#email').value==='fixture@example.invalid'){document.querySelector('dialog').close();"
    "let a=document.createElement('a');a.id='direct';a.href='/v3.0/inapp/login-new?ctx=fixture';a.innerHTML='<i>→</i>';document.querySelector('main').append(a);a.firstChild.click();true}",
    "!!document.querySelector('dialog[open]') && location.pathname==='/v3.0/inapp/loginbyemail' && !switched && sent===1 && recovered===1",
};

static void checked(GObject *object, GAsyncResult *result, gpointer unused) {
    (void)unused;
    GError *error = NULL;
    JSCValue *value = webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(object),result,&error);
    if (value && jsc_value_is_boolean(value) && jsc_value_to_boolean(value)) {
        if (++stage == (fixture ? G_N_ELEMENTS(fixture_checks) : G_N_ELEMENTS(live_checks))) {
            passed = TRUE;
            g_print("MOONTON_CODE_ONLY_OK %s\n", fixture ? "native arrow, keyboard, fallback, adjacent controls" : "publisher form, on-page notice, input preserved");
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
        webkit_web_view_evaluate_javascript(view,fixture ? fixture_checks[stage] : live_checks[stage],-1,NULL,NULL,NULL,checked,NULL);
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
    if (argc != 2 && !(argc == 3 && !g_strcmp0(argv[2], "--fixture"))) return 2;
    fixture = argc == 3;
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
    if (fixture) webkit_web_view_load_html(view,fixture_html,"https://mtacc.mobilelegends.com/v3.0/inapp/loginbyemail");
    else webkit_web_view_load_uri(view,"https://mtacc.mobilelegends.com/");
    g_timeout_add(250,inspect,view);
    g_timeout_add_seconds(45,expired,NULL);
    g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));
    g_object_unref(session);
    g_main_loop_unref(loop);
    return passed ? 0 : 1;
}
