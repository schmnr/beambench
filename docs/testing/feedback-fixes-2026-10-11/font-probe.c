
#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <stdio.h>
static int failed = 0;
static gboolean finished(gpointer data) { puts("PROBE SURVIVED"); gtk_main_quit(); return FALSE; }
static void terminated(WebKitWebView *view, WebKitWebProcessTerminationReason reason, gpointer data) { failed = 1; printf("WEB PROCESS TERMINATED %d\n", reason); gtk_main_quit(); }
int main(int argc, char **argv) {
 gtk_init(&argc, &argv);
 printf("WebKit %u.%u.%u\n",webkit_get_major_version(),webkit_get_minor_version(),webkit_get_micro_version());
 GtkWidget *window=gtk_window_new(GTK_WINDOW_TOPLEVEL);
 GtkWidget *view=webkit_web_view_new();
 gtk_container_add(GTK_CONTAINER(window),view);
 g_signal_connect(view,"web-process-terminated",G_CALLBACK(terminated),NULL);
 webkit_web_view_load_html(WEBKIT_WEB_VIEW(view), "<style>body{font-family: \"Noto Color Emoji\", sans-serif;font-size:48px}</style>Plain ASCII &#x1F680; &#x1F600; &#x1F308; &#x1FAEA;",NULL);
 gtk_widget_show_all(window);
 g_timeout_add_seconds(10,finished,NULL);
 gtk_main();
 return failed ? 1 : 0;
}
