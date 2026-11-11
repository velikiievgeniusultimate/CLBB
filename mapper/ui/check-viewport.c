/* Standalone regression test: no Android, Waydroid, mapper core or extra deps. */
#include "clbb-viewport.h"
#include <math.h>

static void destroyed(gpointer data, GObject *object)
{
    (void)object;
    *(gboolean *)data = TRUE;
}

static void wait_size(GtkWidget *widget, int width, int height)
{
    gint64 deadline = g_get_monotonic_time() + 3000000;
    do {
        while (g_main_context_iteration(NULL, FALSE)) {}
        if (gtk_widget_get_width(widget) == width && gtk_widget_get_height(widget) == height)
            return;
        g_usleep(1000);
    } while (g_get_monotonic_time() < deadline);
    g_error("viewport allocation is %dx%d, expected %dx%d",
            gtk_widget_get_width(widget), gtk_widget_get_height(widget), width, height);
}
static void close_enough(double actual, double expected)
{
    g_assert_cmpfloat(fabs(actual - expected), <, 0.003);
}
static void check_allocation(GtkWidget *viewport, GtkWidget *stack,
                              GtkWidget *activity, int width, int height,
                              int logical_width, int logical_height)
{
    wait_size(viewport, width, height);
    wait_size(stack, logical_width, logical_height);
    wait_size(activity, logical_width, logical_height);
    /* Logical children retain native coordinates even when physically shrunk. */
    g_assert_cmpint(gtk_widget_get_width(stack), ==, logical_width);
    g_assert_cmpint(gtk_widget_get_height(stack), ==, logical_height);
    g_assert_cmpint(gtk_widget_get_width(activity), ==, logical_width);
    g_assert_cmpint(gtk_widget_get_height(activity), ==, logical_height);
    int reported_w = 0, reported_h = 0;
    clbb_viewport_get_resolution(viewport, &reported_w, &reported_h);
    g_assert_cmpint(reported_w, ==, logical_width);
    g_assert_cmpint(reported_h, ==, logical_height);
    const double scale = MIN((double)width/logical_width, (double)height/logical_height);
    graphene_rect_t rect;
    g_assert_true(clbb_viewport_get_content_rect(viewport, &rect));
    close_enough(rect.origin.x, (width-logical_width*scale)/2.);
    close_enough(rect.origin.y, (height-logical_height*scale)/2.);
    close_enough(rect.size.width, logical_width*scale);
    close_enough(rect.size.height, logical_height*scale);
    g_assert_cmpfloat(rect.origin.x, >=, 0.);
    g_assert_cmpfloat(rect.origin.y, >=, 0.);
    g_assert_cmpfloat(rect.origin.x+rect.size.width, <=, width+0.001);
    g_assert_cmpfloat(rect.origin.y+rect.size.height, <=, height+0.001);
    for (int i=0; i<3; ++i) {
        graphene_point_t native = GRAPHENE_POINT_INIT(
            (i==0 ? 0.01 : i==1 ? 0.31 : 0.99)*logical_width,
            (i==0 ? 0.02 : i==1 ? 0.73 : 0.98)*logical_height);
        graphene_point_t physical, round_trip;
        g_assert_true(gtk_widget_compute_point(activity, viewport, &native, &physical));
        close_enough(physical.x, rect.origin.x+native.x*scale);
        close_enough(physical.y, rect.origin.y+native.y*scale);
        g_assert_true(gtk_widget_compute_point(viewport, activity, &physical, &round_trip));
        close_enough(round_trip.x, native.x);
        close_enough(round_trip.y, native.y);
        g_assert_true(gtk_widget_pick(viewport, physical.x, physical.y, GTK_PICK_DEFAULT) == activity);
    }
    if (rect.origin.x>1.)
        g_assert_true(gtk_widget_pick(viewport, 0.5, height/2., GTK_PICK_DEFAULT) == viewport);
    if (rect.origin.y>1.)
        g_assert_true(gtk_widget_pick(viewport, width/2., 0.5, GTK_PICK_DEFAULT) == viewport);
    GdkRectangle fence;
    g_assert_true(clbb_viewport_get_surface_content_rect(viewport,&fence));
    graphene_point_t native_origin;
    GtkNative *root=gtk_widget_get_native(viewport);
    g_assert_true(gtk_widget_compute_point(viewport,GTK_WIDGET(root),&rect.origin,&native_origin));
    double ox=0,oy=0;gtk_native_get_surface_transform(root,&ox,&oy);
    int factor=gdk_surface_get_scale_factor(gtk_native_get_surface(root));
    g_assert_cmpint(fence.x,==,(int)ceil((native_origin.x+ox)*factor));
    g_assert_cmpint(fence.y,==,(int)ceil((native_origin.y+oy)*factor));
    g_assert_cmpint(fence.width,<=,(int)ceil(rect.size.width*factor));
    g_assert_cmpint(fence.height,<=,(int)ceil(rect.size.height*factor));
    g_print("CLBB_VIEWPORT_OK physical=%dx%d logical=%dx%d content=%.3f,%.3f %.3fx%.3f\n",
            width,height,logical_width,logical_height,rect.origin.x,rect.origin.y,rect.size.width,rect.size.height);
}
int main(void)
{
    gtk_init();
    GtkWidget *stack = gtk_stack_new();
    GtkWidget *activity = gtk_drawing_area_new();
    gtk_widget_set_hexpand(activity, TRUE);
    gtk_widget_set_vexpand(activity, TRUE);
    gtk_widget_set_size_request(activity, 4000, 3111);
    gtk_stack_add_named(GTK_STACK(stack), activity, "first-activity");
    GtkWidget *viewport = clbb_viewport_new(stack, 4000, 3111);
    int minimum, natural;
    gtk_widget_measure(viewport, GTK_ORIENTATION_HORIZONTAL, -1, &minimum, &natural, NULL, NULL);
    g_assert_cmpint(minimum, ==, 320);g_assert_cmpint(natural, ==, 960);
    gtk_widget_measure(viewport, GTK_ORIENTATION_VERTICAL, -1, &minimum, &natural, NULL, NULL);
    g_assert_cmpint(minimum, ==, 240);g_assert_cmpint(natural, ==, 540);
    GtkWidget *window = gtk_window_new();
    gtk_window_set_decorated(GTK_WINDOW(window), FALSE);
    gtk_window_set_child(GTK_WINDOW(window), viewport);
    gtk_window_set_default_size(GTK_WINDOW(window), 800, 600);
    gtk_window_present(GTK_WINDOW(window));
    check_allocation(viewport,stack,activity,800,600,4000,3111);
    gtk_window_set_default_size(GTK_WINDOW(window),1920,1080);
    check_allocation(viewport,stack,activity,1920,1080,4000,3111);
    /* Activity replacement, as Android starts another activity in the stack. */
    GtkWidget *next = gtk_drawing_area_new();
    gtk_widget_set_hexpand(next, TRUE);gtk_widget_set_vexpand(next, TRUE);
    gtk_stack_add_named(GTK_STACK(stack),next,"next-activity");
    gtk_stack_set_visible_child(GTK_STACK(stack),next);
    while(g_main_context_iteration(NULL,FALSE)) {}
    check_allocation(viewport,stack,next,1920,1080,4000,3111);
    /* Landscape logical mode yields top/bottom rather than side letterboxing. */
    gtk_widget_set_size_request(activity,-1,-1);
    clbb_viewport_set_resolution(viewport,1920,1080);
    gtk_window_set_default_size(GTK_WINDOW(window),800,600);
    check_allocation(viewport,stack,next,800,600,1920,1080);
    /* Window title/header controls never belong to the game fence. */
    GtkWidget *framed=gtk_window_new();gtk_window_set_decorated(GTK_WINDOW(framed),FALSE);
    GtkWidget *column=gtk_box_new(GTK_ORIENTATION_VERTICAL,0);
    GtkWidget *header=gtk_drawing_area_new();gtk_widget_set_size_request(header,-1,48);
    GtkWidget *content=clbb_viewport_new(gtk_drawing_area_new(),4000,3111);
    gtk_box_append(GTK_BOX(column),header);gtk_box_append(GTK_BOX(column),content);
    gtk_window_set_child(GTK_WINDOW(framed),column);gtk_window_set_default_size(GTK_WINDOW(framed),800,600);
    gtk_window_present(GTK_WINDOW(framed));wait_size(content,800,552);
    GdkRectangle confined;g_assert_true(clbb_viewport_get_surface_content_rect(content,&confined));
    g_assert_cmpint(confined.y,>=,48);g_assert_cmpint(confined.x,>,0);
    g_assert_cmpint(confined.y+confined.height,<=,600);
    g_print("CLBB_FENCE_HEADER_OK rect=%d,%d %dx%d\n",confined.x,confined.y,confined.width,confined.height);
    gtk_window_destroy(GTK_WINDOW(framed));
    gboolean viewport_gone=FALSE, stack_gone=FALSE, first_gone=FALSE, next_gone=FALSE;
    g_object_weak_ref(G_OBJECT(viewport),destroyed,&viewport_gone);
    g_object_weak_ref(G_OBJECT(stack),destroyed,&stack_gone);
    g_object_weak_ref(G_OBJECT(activity),destroyed,&first_gone);
    g_object_weak_ref(G_OBJECT(next),destroyed,&next_gone);
    gtk_window_destroy(GTK_WINDOW(window));
    while(g_main_context_iteration(NULL,FALSE)) {}
    g_assert_true(viewport_gone);g_assert_true(stack_gone);
    g_assert_true(first_gone);g_assert_true(next_gone);
    g_print("CLBB_VIEWPORT_DISPOSE_OK\n");
    return 0;
}
