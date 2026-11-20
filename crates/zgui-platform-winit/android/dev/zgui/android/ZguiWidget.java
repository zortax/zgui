package dev.zgui.android;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.widget.RemoteViews;

/** Home screen widget with one line of text. Tapping it opens the application with payload
 * "widget".
 */
public final class ZguiWidget extends AppWidgetProvider {
    @Override public void onUpdate(Context context, AppWidgetManager manager, int[] ids) {
        update(context, manager, ids);
    }

    static void refresh(Context context) {
        AppWidgetManager manager = AppWidgetManager.getInstance(context);
        update(context, manager, manager.getAppWidgetIds(new ComponentName(context, ZguiWidget.class)));
    }

    private static void update(Context context, AppWidgetManager manager, int[] ids) {
        String label = context.getApplicationInfo().loadLabel(context.getPackageManager()).toString();
        String text = Zgui.prefs(context).getString("widget.text", label);
        for (int id : ids) {
            // Platform layout: the application needs no layout resource of its own.
            RemoteViews view = new RemoteViews("android", android.R.layout.simple_list_item_1);
            view.setTextViewText(android.R.id.text1, text);
            view.setOnClickPendingIntent(android.R.id.text1, PendingIntent.getActivity(context, id,
                Zgui.open(context, "widget"), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE));
            manager.updateAppWidget(id, view);
        }
    }
}
