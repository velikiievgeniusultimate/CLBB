#include <gio/gio.h>
#include <stdio.h>
int main(void) {
 GError *error=NULL;
 gchar **uris=g_proxy_resolver_lookup(g_proxy_resolver_get_default(),"https://accountmtapi.mobilelegends.com/",NULL,&error);
 if (!uris) { printf("RESOLVER_ERROR domain=%s code=%d\n",error?g_quark_to_string(error->domain):"none",error?error->code:0); g_clear_error(&error); return 1; }
 for (int i=0; uris[i]; ++i) {
  GUri *uri=g_uri_parse(uris[i],G_URI_FLAGS_PARSE_RELAXED,NULL);
  const char *scheme=uri?g_uri_get_scheme(uri):NULL;
  printf("RESOLVER_RESULT scheme=%s port=%d auth=%d\n",scheme?scheme:"none",uri?g_uri_get_port(uri):-1,uri&&g_uri_get_userinfo(uri)?1:0);
  if(uri)g_uri_unref(uri);
 }
 g_strfreev(uris); return 0;
}
