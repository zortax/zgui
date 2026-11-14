package dev.zgui.android;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.hardware.usb.UsbManager;

/** Delivers alarms, restart, notification actions and USB grants. Background events run the
 * Rust `android_background!` handler on a worker thread.
 */
public final class ZguiReceiver extends BroadcastReceiver {
    static final String ALARM = "dev.zgui.ALARM", ACTION = "dev.zgui.ACTION", USB = "dev.zgui.USB";

    @Override public void onReceive(Context context, Intent intent) {
        Zgui.init(context);
        String action = String.valueOf(intent.getAction());
        switch (action) {
        case Intent.ACTION_BOOT_COMPLETED:
            Zgui.reschedule(context);
            dispatch(context, "boot");
            break;
        case ALARM: {
            int id = intent.getIntExtra("id", 0);
            Zgui.prefs(context).edit().remove("alarm." + id).apply();
            dispatch(context, "alarm", Integer.toString(id));
            break;
        }
        case ACTION:
            dispatch(context, "action", Integer.toString(intent.getIntExtra("id", 0)),
                Zgui.str(intent.getStringExtra("action")), Zgui.str(intent.getStringExtra("payload")));
            break;
        case USB:
            Zgui.ok(intent.getLongExtra("token", 0),
                Zgui.bool(intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)));
            break;
        default:
        }
    }

    private void dispatch(Context context, String kind, String... args) {
        PendingResult pending = goAsync();
        Zgui.io.execute(() -> {
            try { Zgui.dispatch(context, kind, args); } finally { pending.finish(); }
        });
    }
}
