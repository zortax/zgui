package dev.zgui.android;

import android.app.AlarmManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.job.JobInfo;
import android.app.job.JobScheduler;
import android.appwidget.AppWidgetManager;
import android.content.ClipData;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.content.pm.ShortcutInfo;
import android.content.pm.ShortcutManager;
import android.graphics.drawable.Icon;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.util.Log;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Entry points that Rust, the activity host and the packaged components share.
 * Rust calls {@link #call} with an operation name, a reply token and string arguments.
 * Results return through {@link #reply}; ongoing operations stop through "cancel".
 */
public final class Zgui {
    static final int OK = 0, CANCELLED = 1, ERROR = 2;
    static final String PAYLOAD = "dev.zgui.payload";
    static final String NOTIFICATIONS = "android.permission.POST_NOTIFICATIONS";
    static final Handler main = new Handler(Looper.getMainLooper());
    static final ExecutorService io = Executors.newCachedThreadPool();
    /** Stops an ongoing operation, by reply token. */
    static final Map<Long, Runnable> active = new ConcurrentHashMap<>();
    static volatile Context context;
    /** Host of the visible activity in this process, or null. */
    static volatile ZguiHost host;
    private static final List<String> intents = new ArrayList<>();
    private static boolean loaded;

    static native void reply(long token, boolean last, int code, String[] text, byte[] bytes);
    static native void event(String kind, String[] text);
    static native void pointer(int action, int id, float x, float y, float pressure, int tool, int modifiers, int button);
    static native void key(boolean down, String text, String plain, int scan, int modifiers, boolean repeat);
    static native void wheel(float x, float y, float dx, float dy, int modifiers);
    /** Defined by the application with the `android_background!` macro. */
    private static native void background(Context context, String kind, String[] args);

    private Zgui() {}

    public static void init(Context from) {
        if (context == null) context = from.getApplicationContext();
    }

    static SharedPreferences prefs(Context c) { return c.getSharedPreferences("zgui", Context.MODE_PRIVATE); }

    static void send(long token, boolean last, int code, String[] text, byte[] bytes) {
        if (last) active.remove(token);
        try {
            reply(token, last, code, text == null ? new String[0] : text, bytes);
        } catch (UnsatisfiedLinkError detached) {
            Log.w("zgui", "Reply without Rust attached", detached);
        }
    }
    static void ok(long token, String... text) { send(token, true, OK, text, null); }
    static void next(long token, String... text) { send(token, false, OK, text, null); }
    static void cancelled(long token) { send(token, true, CANCELLED, null, null); }
    static void fail(long token, Throwable error) { send(token, true, ERROR, new String[] {describe(error)}, null); }
    static String describe(Throwable error) {
        String message = error.getMessage();
        return message == null ? error.getClass().getSimpleName() : message;
    }
    static String[] bool(boolean value) { return new String[] {value ? "1" : "0"}; }
    static String str(Object value) { return value == null ? "" : value.toString(); }

    interface Task { void run() throws Exception; }

    /** Runs `task` on the main thread. A failure ends the operation of `token`. */
    static void onMain(long token, Task task) {
        main.post(() -> {
            try { task.run(); } catch (Exception error) { fail(token, error); }
        });
    }

    /** Permission Android checks for `permission` on this version; null when it is implicit. */
    static String effective(String permission) {
        if (Build.VERSION.SDK_INT < 33 && permission.equals(NOTIFICATIONS)) return null;
        if (Build.VERSION.SDK_INT < 31 && permission.equals("android.permission.BLUETOOTH_SCAN"))
            return "android.permission.ACCESS_FINE_LOCATION";
        if (Build.VERSION.SDK_INT < 31 && permission.equals("android.permission.BLUETOOTH_CONNECT")) return null;
        return permission;
    }
    static boolean granted(Context c, String permission) {
        String checked = effective(permission);
        return checked == null || c.checkSelfPermission(checked) == PackageManager.PERMISSION_GRANTED;
    }

    /** Called from Rust on any thread. */
    public static String[] call(String op, long token, String[] a) throws Exception {
        Context c = context;
        if (c == null) throw new IllegalStateException("Android context unavailable");
        if (op.startsWith("host.")) {
            ZguiHost visible = host;
            if (visible == null) throw new IllegalStateException("No activity in this process");
            return visible.call(op, token, a);
        }
        switch (op) {
        case "cancel": {
            Runnable stop = active.remove(token);
            if (stop != null) main.post(stop);
            return null;
        }
        case "granted": return bool(granted(c, a[0]));
        case "intents": return takeIntents();
        case "files":
            return new String[] {c.getFilesDir().getAbsolutePath(), c.getCacheDir().getAbsolutePath(),
                ZguiFiles.dir(c).getAbsolutePath()};
        case "locale": return new String[] {Locale.getDefault().toLanguageTag()};
        case "notify":
            if (!granted(c, NOTIFICATIONS)) throw new SecurityException("Notification permission not granted");
            c.getSystemService(NotificationManager.class).notify(Integer.parseInt(a[0]), build(c, a));
            return null;
        case "notify.cancel":
            c.getSystemService(NotificationManager.class).cancel(Integer.parseInt(a[0]));
            return null;
        case "alarm":
            prefs(c).edit().putLong("alarm." + a[0], Long.parseLong(a[1])).apply();
            schedule(c, Integer.parseInt(a[0]), Long.parseLong(a[1]));
            return null;
        case "alarm.cancel":
            prefs(c).edit().remove("alarm." + a[0]).apply();
            c.getSystemService(AlarmManager.class).cancel(alarmIntent(c, Integer.parseInt(a[0])));
            return null;
        case "job": job(c, a); return null;
        case "job.cancel": c.getSystemService(JobScheduler.class).cancel(Integer.parseInt(a[0])); return null;
        case "service": service(c, a); return null;
        case "service.stop": {
            ZguiService running = ZguiService.current;
            if (running != null) main.post(running::finish);
            else c.stopService(new Intent(c, ZguiService.class));
            return null;
        }
        case "content":
            return new String[] {Integer.toString(c.getContentResolver().openFileDescriptor(Uri.parse(a[0]), a[1]).detachFd())};
        case "share.file": shareFile(c, a[0], a[1]); return null;
        case "settings": settings(c, a[0]); return null;
        case "shortcut": shortcut(c, a); return null;
        case "widget.pin": {
            AppWidgetManager widgets = c.getSystemService(AppWidgetManager.class);
            if (!widgets.isRequestPinAppWidgetSupported()) throw new UnsupportedOperationException("Launcher does not support pinned widgets");
            widgets.requestPinAppWidget(new ComponentName(c, ZguiWidget.class), null, null);
            return null;
        }
        case "widget.text":
            prefs(c).edit().putString("widget.text", a[0]).apply();
            ZguiWidget.refresh(c);
            return null;
        default: return ZguiDevice.call(c, op, token, a);
        }
    }

    /** Called from Rust on any thread, for operations on bytes. */
    public static byte[] bytes(String op, String[] a, byte[] data) throws Exception {
        return ZguiDevice.bytes(op, a, data);
    }

    /** Queues an incoming intent for Rust; Rust drains the queue through "intents". */
    @SuppressWarnings("deprecation")
    static void intent(Intent intent) {
        if (intent == null) return;
        Object text = null, stream = null;
        if (Intent.ACTION_SEND.equals(intent.getAction())) {
            text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT);
            stream = intent.getParcelableExtra(Intent.EXTRA_STREAM);
        }
        String[] item = {str(intent.getAction()), str(intent.getDataString()), str(text), str(stream),
            str(intent.getType()), str(intent.getStringExtra(PAYLOAD))};
        synchronized (intents) { intents.addAll(Arrays.asList(item)); }
        try { event("intent", new String[0]); } catch (UnsatisfiedLinkError notAttached) { /* Rust drains later. */ }
    }
    private static String[] takeIntents() {
        synchronized (intents) {
            String[] all = intents.toArray(new String[0]);
            intents.clear();
            return all;
        }
    }

    /** Opens the launcher activity with `payload`, delivered to Rust as an intent. */
    static Intent open(Context c, String payload) {
        Intent launch = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        return new Intent("dev.zgui.OPEN").setComponent(launch.getComponent())
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP)
            .putExtra(PAYLOAD, payload);
    }

    /** a: id, title, text, payload, ongoing, then action id and label pairs. */
    static Notification build(Context c, String[] a) {
        int id = Integer.parseInt(a[0]);
        boolean ongoing = "1".equals(a[4]);
        String channel = ongoing ? "zgui-ongoing" : "zgui";
        c.getSystemService(NotificationManager.class).createNotificationChannel(new NotificationChannel(channel,
            ongoing ? "Ongoing work" : "Notifications",
            ongoing ? NotificationManager.IMPORTANCE_LOW : NotificationManager.IMPORTANCE_HIGH));
        PendingIntent tap = PendingIntent.getActivity(c, id, open(c, a[3]),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification.Builder builder = new Notification.Builder(c, channel)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(a[1]).setContentText(a[2]).setStyle(new Notification.BigTextStyle().bigText(a[2]))
            .setContentIntent(tap).setOngoing(ongoing).setAutoCancel(!ongoing).setOnlyAlertOnce(true);
        int icon = c.getApplicationInfo().icon;
        if (icon != 0) builder.setLargeIcon(Icon.createWithResource(c, icon));
        for (int i = 5; i + 1 < a.length; i += 2) {
            Intent action = new Intent(c, ZguiReceiver.class).setAction(ZguiReceiver.ACTION)
                .putExtra("id", id).putExtra("action", a[i]).putExtra("payload", a[3]);
            PendingIntent pending = PendingIntent.getBroadcast(c, id * 8 + (i - 5) / 2, action,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            builder.addAction(new Notification.Action.Builder(null, a[i + 1], pending).build());
        }
        return builder.build();
    }

    static PendingIntent alarmIntent(Context c, int id) {
        Intent alarm = new Intent(c, ZguiReceiver.class).setAction(ZguiReceiver.ALARM).putExtra("id", id);
        return PendingIntent.getBroadcast(c, id, alarm, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }
    /** Exact when the system allows it, otherwise inexact. */
    static void schedule(Context c, int id, long at) {
        AlarmManager alarms = c.getSystemService(AlarmManager.class);
        long when = Math.max(at, System.currentTimeMillis());
        if (Build.VERSION.SDK_INT >= 31 && !alarms.canScheduleExactAlarms())
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, when, alarmIntent(c, id));
        else alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, when, alarmIntent(c, id));
    }
    /** Android clears alarms on restart; the receiver restores them from preferences. */
    static void reschedule(Context c) {
        for (Map.Entry<String, ?> entry : prefs(c).getAll().entrySet()) {
            if (entry.getKey().startsWith("alarm.") && entry.getValue() instanceof Long)
                schedule(c, Integer.parseInt(entry.getKey().substring(6)), (Long)entry.getValue());
        }
    }

    /** a: id, delay ms, deadline ms or -1, network type, charging, persisted. */
    private static void job(Context c, String[] a) {
        JobInfo.Builder builder = new JobInfo.Builder(Integer.parseInt(a[0]), new ComponentName(c, ZguiJobs.class))
            .setMinimumLatency(Long.parseLong(a[1]))
            .setRequiredNetworkType(Integer.parseInt(a[3]))
            .setRequiresCharging("1".equals(a[4]))
            .setPersisted("1".equals(a[5]));
        long deadline = Long.parseLong(a[2]);
        // JobScheduler rejects a job without any constraint.
        if (deadline >= 0 || (Long.parseLong(a[1]) == 0 && "0".equals(a[3]) && !"1".equals(a[4])))
            builder.setOverrideDeadline(Math.max(deadline, 0));
        if (c.getSystemService(JobScheduler.class).schedule(builder.build()) != JobScheduler.RESULT_SUCCESS)
            throw new IllegalStateException("JobScheduler rejected job");
    }

    /** a: action, payload, foreground, then notification arguments. */
    private static void service(Context c, String[] a) {
        Intent intent = new Intent(c, ZguiService.class).setAction(a[0]).putExtra("payload", a[1]);
        if ("1".equals(a[2])) {
            intent.putExtra("notification", Arrays.copyOfRange(a, 3, a.length));
            c.startForegroundService(intent);
        } else {
            c.startService(intent);
        }
    }

    private static void shareFile(Context c, String name, String mime) {
        Uri uri = ZguiFiles.uri(c, name);
        Intent send = new Intent(Intent.ACTION_SEND).setType(mime).putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        send.setClipData(ClipData.newRawUri("", uri));
        c.startActivity(Intent.createChooser(send, null)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_GRANT_READ_URI_PERMISSION));
    }

    private static void settings(Context c, String kind) {
        String pkg = c.getPackageName();
        Intent intent;
        switch (kind) {
        case "notifications":
            intent = new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, pkg);
            break;
        case "battery": intent = new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS); break;
        case "location": intent = new Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS); break;
        case "alarms":
            intent = Build.VERSION.SDK_INT >= 31
                ? new Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, Uri.parse("package:" + pkg))
                : new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + pkg));
            break;
        default: intent = new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + pkg));
        }
        c.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
    }

    /** a: id, label, payload. */
    private static void shortcut(Context c, String[] a) {
        ShortcutManager shortcuts = c.getSystemService(ShortcutManager.class);
        if (!shortcuts.isRequestPinShortcutSupported()) throw new UnsupportedOperationException("Launcher does not support pinned shortcuts");
        int icon = c.getApplicationInfo().icon;
        shortcuts.requestPinShortcut(new ShortcutInfo.Builder(c, a[0]).setShortLabel(a[1]).setIntent(open(c, a[2]))
            .setIcon(Icon.createWithResource(c, icon != 0 ? icon : android.R.drawable.ic_menu_view)).build(), null);
    }

    /** Runs the application's background handler on the calling thread. */
    static void dispatch(Context c, String kind, String... args) {
        init(c);
        try {
            load(c);
            background(context, kind, args);
        } catch (UnsatisfiedLinkError missing) {
            Log.w("zgui", "Application defines no android_background! handler", missing);
        }
    }
    /** Loads the activity's native library, named by its `android.app.lib_name` metadata. */
    private static synchronized void load(Context c) {
        if (loaded) return;
        String lib = "main";
        try {
            PackageManager packages = c.getPackageManager();
            ComponentName launcher = packages.getLaunchIntentForPackage(c.getPackageName()).getComponent();
            Bundle meta = packages.getActivityInfo(launcher, PackageManager.GET_META_DATA).metaData;
            if (meta != null) lib = meta.getString("android.app.lib_name", lib);
        } catch (Exception useDefault) { /* NativeActivity default name. */ }
        System.loadLibrary(lib);
        loaded = true;
    }
}
