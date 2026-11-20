package dev.zgui.android;

import android.app.Notification;
import android.app.Service;
import android.content.Intent;
import android.os.IBinder;
import android.util.Log;

/** Runs application work while no activity is visible. Each start command reaches the Rust
 * `android_background!` handler; a command with a notification makes the service foreground.
 * The manifest entry sets the foreground type and, optionally, a separate process.
 */
public final class ZguiService extends Service {
    /** Service running in this process, or null. */
    static volatile ZguiService current;

    @Override public void onCreate() {
        super.onCreate();
        current = this;
        Zgui.init(this);
    }

    @Override public int onStartCommand(Intent intent, int flags, int id) {
        if (intent == null || intent.getAction() == null) {
            stopSelf();
            return START_NOT_STICKY;
        }
        String[] notification = intent.getStringArrayExtra("notification");
        if (notification != null) {
            try {
                Notification shown = Zgui.build(this, notification);
                startForeground(Integer.parseInt(notification[0]), shown);
            } catch (RuntimeException refused) {
                // Missing foreground type permission, or a start Android does not allow now.
                Log.w("zgui", "Foreground service refused", refused);
                Zgui.dispatch(this, "service.failed", intent.getAction(), Zgui.describe(refused));
                stopSelf();
                return START_NOT_STICKY;
            }
        }
        Zgui.dispatch(this, "service", intent.getAction(), Zgui.str(intent.getStringExtra("payload")));
        return START_NOT_STICKY;
    }

    void finish() {
        stopForeground(STOP_FOREGROUND_REMOVE);
        stopSelf();
    }

    @Override public void onDestroy() {
        current = null;
        Zgui.dispatch(this, "service.stopped");
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
