#include <gtk/gtk.h>
#include <webkit/webkit.h>
#include <stdio.h>
#include <string.h>

struct test { const char *label; WebKitWebView *view; gboolean loaded, gate, fetching, done; guint load_status; };
static struct test tests[] = {{.label="default"},{.label="sdk-suffix"}};
static GMainLoop *loop;
static int result = 1;
static const char *headers[] = {"Server", "Allow", "Access-Control-Allow-Origin", "Access-Control-Allow-Methods", "Access-Control-Allow-Headers", "Access-Control-Allow-Credentials"};
static void maybe_fetch(struct test *t);
static void check_done(void) {
    if (tests[0].done && tests[1].done) { result=0; g_main_loop_quit(loop); }
}
static void response_changed(GObject *object, GParamSpec *pspec, gpointer data) {
    (void)pspec;
    struct test *t=data;
    WebKitURIResponse *response=webkit_web_resource_get_response(WEBKIT_WEB_RESOURCE(object));
    if (!response) return;
    GUri *uri=g_uri_parse(webkit_uri_response_get_uri(response),G_URI_FLAGS_NONE,NULL);
    if (!uri) return;
    gboolean exact=!g_strcmp0(g_uri_get_host(uri),"accountmtapi.mobilelegends.com") && !g_strcmp0(g_uri_get_path(uri),"/") && !g_uri_get_query(uri);
    g_uri_unref(uri);
    if (!exact) return;
    printf("hostname=accountmtapi.mobilelegends.com ua=%s credentials=include wire_status=%u\n",t->label,webkit_uri_response_get_status_code(response));
    SoupMessageHeaders *h=webkit_uri_response_get_http_headers(response);
    for (unsigned i=0;h && i<G_N_ELEMENTS(headers);i++) {
        const char *v=soup_message_headers_get_one(h,headers[i]);
        char *escaped=v?g_strescape(v,NULL):NULL;
        printf("hostname=accountmtapi.mobilelegends.com ua=%s credentials=include header=%s value=%s\n",t->label,headers[i],escaped?escaped:"<absent>");
        g_free(escaped);
    }
    fflush(stdout);
}
static void resource_started(WebKitWebView *view, WebKitWebResource *resource, WebKitURIRequest *request, gpointer data) {
    (void)view; (void)request;
    g_signal_connect(resource,"notify::response",G_CALLBACK(response_changed),data);
}
static void fetched(GObject *object,GAsyncResult *async,gpointer data) {
    struct test *t=data; GError *error=NULL;
    JSCValue *v=webkit_web_view_call_async_javascript_function_finish(WEBKIT_WEB_VIEW(object),async,&error);
    if (v) {
        JSCValue *status=jsc_value_object_get_property(v,"status"), *exception=jsc_value_object_get_property(v,"exception");
        char *name=jsc_value_to_string(exception);
        const char *safe=(!strcmp(name,"none") || !strcmp(name,"TypeError") || !strcmp(name,"AbortError") || !strcmp(name,"SecurityError"))?name:"OtherError";
        printf("hostname=accountmtapi.mobilelegends.com ua=%s credentials=include status=%d exception=%s gerror_domain=none gerror_code=0\n",t->label,jsc_value_to_int32(status),safe);
        g_free(name);g_object_unref(status);g_object_unref(exception);g_object_unref(v);
    } else {
        printf("hostname=accountmtapi.mobilelegends.com ua=%s credentials=include status=0 exception=none gerror_domain=%s gerror_code=%d\n",t->label,error?g_quark_to_string(error->domain):"none",error?error->code:0);
        g_clear_error(&error);
    }
    fflush(stdout); t->done=TRUE; check_done();
}
static void maybe_fetch(struct test *t) {
    if (!t->loaded || !t->gate || t->fetching || t->done) return;
    t->fetching=TRUE;
    if (t->load_status!=200) { t->done=TRUE;check_done();return; }
    webkit_settings_set_enable_javascript(webkit_web_view_get_settings(t->view),TRUE);
    const char *body="const c=new AbortController();const timer=setTimeout(()=>c.abort(),10000);"
        "try {const r=await fetch('https://accountmtapi.mobilelegends.com/',{method:'POST',body:'{}',"
        "headers:{'Content-Type':'application/json'},mode:'cors',credentials:'include',cache:'no-store',redirect:'error',signal:c.signal});"
        "return {status:r.status,exception:'none'};}catch(e){return {status:0,exception:e.name};}finally{clearTimeout(timer);}";
    webkit_web_view_call_async_javascript_function(t->view,body,-1,NULL,NULL,NULL,NULL,fetched,t);
}
static gboolean user_message(WebKitWebView *view,WebKitUserMessage *message,gpointer data) {
    (void)view;struct test*t=data;
    if (strcmp(webkit_user_message_get_name(message),"root-only-gate")) return FALSE;
    t->gate=TRUE;maybe_fetch(t);return TRUE;
}
static void loaded(WebKitWebView *view,WebKitLoadEvent event,gpointer data) {
    struct test*t=data;if(event!=WEBKIT_LOAD_FINISHED||t->loaded)return;
    WebKitWebResource*r=webkit_web_view_get_main_resource(view);
    WebKitURIResponse*s=r?webkit_web_resource_get_response(r):NULL;
    t->load_status=s?webkit_uri_response_get_status_code(s):0;
    printf("hostname=mtacc.mobilelegends.com ua=%s credentials=include status=%u gerror_domain=none gerror_code=0\n",t->label,t->load_status);
    t->loaded=TRUE;maybe_fetch(t);
}
static gboolean failed(WebKitWebView*v,WebKitLoadEvent event,const char*uri,GError*error,gpointer data) {
    (void)v;(void)event;(void)uri;struct test*t=data;
    printf("hostname=mtacc.mobilelegends.com ua=%s credentials=include status=0 gerror_domain=%s gerror_code=%d\n",t->label,g_quark_to_string(error->domain),error->code);
    t->done=TRUE;check_done();return TRUE;
}
static gboolean authenticate(WebKitWebView*v,WebKitAuthenticationRequest*r,gpointer data) {
    (void)v;(void)data;webkit_authentication_request_cancel(r);return TRUE;
}
static gboolean deadline(gpointer data) {
    (void)data;
    for(unsigned i=0;i<G_N_ELEMENTS(tests);i++)if(!tests[i].done)
        printf("hostname=accountmtapi.mobilelegends.com ua=%s credentials=include status=0 gerror_domain=g-io-error-quark gerror_code=%d\n",tests[i].label,G_IO_ERROR_TIMED_OUT);
    fflush(stdout);g_main_loop_quit(loop);return G_SOURCE_REMOVE;
}
int main(int argc,char**argv) {
    if(argc!=3||getenv("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"))return 2;
    gtk_init();loop=g_main_loop_new(NULL,FALSE);
    WebKitWebContext*context=webkit_web_context_new();
    webkit_web_context_add_path_to_sandbox(context,argv[1],TRUE);
    webkit_web_context_add_path_to_sandbox(context,argv[2],TRUE);
    webkit_web_context_set_web_process_extensions_directory(context,argv[2]);
    GtkWidget*window=gtk_window_new(),*box=gtk_box_new(GTK_ORIENTATION_VERTICAL,0);
    gtk_window_set_child(GTK_WINDOW(window),box);gtk_window_set_default_size(GTK_WINDOW(window),640,480);
    for(unsigned i=0;i<G_N_ELEMENTS(tests);i++) {
        struct test*t=&tests[i];WebKitNetworkSession*session=webkit_network_session_new_ephemeral();
        webkit_network_session_set_tls_errors_policy(session,WEBKIT_TLS_ERRORS_POLICY_FAIL);
        webkit_network_session_set_proxy_settings(session,WEBKIT_NETWORK_PROXY_MODE_NO_PROXY,NULL);
        t->view=WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,"web-context",context,"network-session",session,NULL));
        g_object_unref(session);
        WebKitSettings*settings=webkit_web_view_get_settings(t->view);
        webkit_settings_set_enable_javascript(settings,FALSE);
        if(i) {char*ua=g_strconcat(webkit_settings_get_user_agent(settings),";UniWebView;mtakit",NULL);webkit_settings_set_user_agent(settings,ua);g_free(ua);}
        g_signal_connect(t->view,"load-changed",G_CALLBACK(loaded),t);
        g_signal_connect(t->view,"load-failed",G_CALLBACK(failed),t);
        g_signal_connect(t->view,"user-message-received",G_CALLBACK(user_message),t);
        g_signal_connect(t->view,"resource-load-started",G_CALLBACK(resource_started),t);
        g_signal_connect(t->view,"authenticate",G_CALLBACK(authenticate),NULL);
        gtk_widget_set_vexpand(GTK_WIDGET(t->view),TRUE);gtk_box_append(GTK_BOX(box),GTK_WIDGET(t->view));
    }
    gtk_window_present(GTK_WINDOW(window));
    for(unsigned i=0;i<G_N_ELEMENTS(tests);i++)webkit_web_view_load_uri(tests[i].view,"https://mtacc.mobilelegends.com/");
    g_timeout_add_seconds(25,deadline,NULL);g_main_loop_run(loop);
    gtk_window_destroy(GTK_WINDOW(window));g_object_unref(context);g_main_loop_unref(loop);return result;
}
