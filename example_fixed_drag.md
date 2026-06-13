## example begin and update
```c
static void on_drag_begin(GtkGestureDrag *gesture, double x, double y, gpointer user_data) {
    GtkFixed *fixed = GTK_FIXED(user_data);
    // x, yはFixed基準。ここから対象の子ウィジェットを特定
    target = find_child_at(fixed, x, y);
}

static void on_drag_update(GtkGestureDrag *gesture, double offset_x, double offset_y, gpointer user_data) {
    GtkFixed *fixed = GTK_FIXED(user_data);
    // targetの元の位置 + offset で新しい位置を計算
    gtk_fixed_move(fixed, target, orig_x + offset_x, orig_y + offset_y);
}
```

## check which is whether draggale
```c
// draggableにしたい/したくない要素にセット
g_object_set_data(G_OBJECT(child), "draggable", GINT_TO_POINTER(TRUE));


static void on_drag_begin(GtkGestureDrag *gesture, double x, double y, gpointer user_data) {
    GtkFixed *fixed = GTK_FIXED(user_data);
    GtkWidget *child = find_child_at(fixed, x, y);

    if (!child || !g_object_get_data(G_OBJECT(child), "draggable")) {
        target = NULL; // ドラッグしない
        return;
    }
    target = child;
}
```
