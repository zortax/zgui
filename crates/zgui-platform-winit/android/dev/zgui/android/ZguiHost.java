package dev.zgui.android;

import android.app.Activity;
import android.app.Application;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.ComponentCallbacks;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.res.Configuration;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.net.Uri;
import android.view.HapticFeedbackConstants;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import android.content.Context;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.hardware.display.DisplayManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.PersistableBundle;
import android.os.SystemClock;
import android.text.Editable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.view.Display;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager;
import android.widget.EditText;
import android.widget.FrameLayout;

/** Android services for a ZGUI NativeActivity. Rust loads this class from memory.
 * Transparent editor supplies Android text editing for the focused ZGUI field; ZGUI paints text.
 */
public final class ZguiHost
        implements Application.ActivityLifecycleCallbacks, DisplayManager.DisplayListener,
            ComponentCallbacks, SensorEventListener {
    private static final String CHANNEL = "zgui";
    private static final String NOTIFY_PERMISSION = "android.permission.POST_NOTIFICATIONS";

    private final Activity activity;
    private final Handler main = new Handler(Looper.getMainLooper());
    private HostEditor editor;
    private long surface, target, revision;
    private boolean syncing, queued;
    private long touchDown;
    private int notifications;
    private Runnable pendingNotification;

    private static native void changed(long surface, long target, long revision,
                                       String text, int anchor, int focus);
    private static native void destroyed();
    private static native void back();
    private android.window.OnBackInvokedCallback backCallback;
    private static native void insetsChanged(int top, int right, int bottom, int left, int keyboard);
    private static native void refreshRate(float hertz);
    private static native void configurationChanged(boolean dark, float fontScale, boolean rtl);
    private static native void tilted(float x, float y);
    private static native void picked(String path);

    private boolean tilting, askedSensors;
    /** Theme the application shows: 0 follows the system, 1 light, 2 dark. */
    private int barTheme;
    /** GrapheneOS permission for sensors stock Android leaves open. */
    private static final String SENSOR_PERMISSION = "android.permission.OTHER_SENSORS";
    private int pickedImages;

    /** fontPath names the font file the editor measures with; null uses the system font. */
    public ZguiHost(Activity activity, String fontPath) {
        this.activity = activity;
        Zgui.init(activity);
        Zgui.host = this;
        activity.runOnUiThread(() -> attach(fontPath));
    }

    /** Operations that need the visible activity. Called by {@link Zgui#call} on any thread. */
    String[] call(String op, long token, String[] a) throws Exception {
        switch (op) {
        case "host.permissions": Zgui.onMain(token, () -> permissions(token, a)); return null;
        case "host.open": Zgui.onMain(token, () -> openDocument(token, a[0])); return null;
        case "host.create": Zgui.onMain(token, () -> createDocument(token, a[0], a[1])); return null;
        case "host.camera": Zgui.onMain(token, () -> camera(token)); return null;
        case "host.account":
            Zgui.onMain(token, () -> forResult(token, android.accounts.AccountManager.newChooseAccountIntent(
                null, null, null, null, null, null, null), (code, data) -> {
                    if (code == Activity.RESULT_OK && data != null)
                        Zgui.ok(token, Zgui.str(data.getStringExtra(android.accounts.AccountManager.KEY_ACCOUNT_NAME)));
                    else Zgui.cancelled(token);
                }));
            return null;
        case "host.biometric": Zgui.onMain(token, () -> biometric(token, a[0], a[1], a[2])); return null;
        case "host.nfc": Zgui.onMain(token, () -> nfc(token)); return null;
        case "host.pip": {
            if (!activity.getPackageManager().hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE))
                throw new UnsupportedOperationException("Picture in picture unsupported");
            android.app.PictureInPictureParams params = new android.app.PictureInPictureParams.Builder()
                .setAspectRatio(new android.util.Rational(Integer.parseInt(a[0]), Integer.parseInt(a[1]))).build();
            main.post(() -> {
                try { activity.enterPictureInPictureMode(params); }
                catch (IllegalStateException notDeclared) { android.util.Log.w("zgui", "Picture in picture refused", notDeclared); }
            });
            return null;
        }
        case "host.window":
            return new String[] {Zgui.bool(activity.isInMultiWindowMode())[0], Zgui.bool(activity.isInPictureInPictureMode())[0]};
        default: throw new IllegalArgumentException("Unknown operation " + op);
        }
    }

    /** Receives one activity or permission result. NativeActivity has no result callbacks. */
    @SuppressWarnings("deprecation")
    public static final class Request extends android.app.Fragment {
        interface Done { void result(int code, Intent data) throws Exception; }
        long token;
        Done done;
        @Override public void onActivityResult(int request, int code, Intent data) {
            finish();
            try { if (done != null) done.result(code, data); } catch (Exception error) { Zgui.fail(token, error); }
        }
        @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
            finish();
            boolean all = results.length > 0;
            for (int result : results) all &= result == PackageManager.PERMISSION_GRANTED;
            Zgui.ok(token, Zgui.bool(all));
        }
        void finish() { getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss(); }
    }

    @SuppressWarnings("deprecation")
    private Request request(long token) {
        Request request = new Request();
        request.token = token;
        activity.getFragmentManager().beginTransaction().add(request, "zgui-" + token).commitNow();
        return request;
    }

    @SuppressWarnings("deprecation")
    private void forResult(long token, Intent intent, Request.Done done) {
        Request request = request(token);
        request.done = done;
        try {
            request.startActivityForResult(intent, 1);
        } catch (ActivityNotFoundException missing) {
            request.finish();
            Zgui.fail(token, missing);
        }
    }

    @SuppressWarnings("deprecation")
    private void permissions(long token, String[] asked) {
        java.util.List<String> missing = new java.util.ArrayList<>();
        for (String permission : asked) {
            String checked = Zgui.effective(permission);
            if (checked != null && activity.checkSelfPermission(checked) != PackageManager.PERMISSION_GRANTED) missing.add(checked);
        }
        if (missing.isEmpty()) { Zgui.ok(token, "1"); return; }
        request(token).requestPermissions(missing.toArray(new String[0]), 1);
    }

    private void openDocument(long token, String mime) {
        forResult(token, new Intent(Intent.ACTION_OPEN_DOCUMENT).setType(mime).addCategory(Intent.CATEGORY_OPENABLE), (code, data) -> {
            if (code != Activity.RESULT_OK || data == null || data.getData() == null) { Zgui.cancelled(token); return; }
            Uri uri = data.getData();
            try {
                activity.getContentResolver().takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
            } catch (SecurityException notPersistable) { /* Access lasts while the process lives. */ }
            Zgui.ok(token, uri.toString(), Integer.toString(activity.getContentResolver().openFileDescriptor(uri, "r").detachFd()));
        });
    }

    private void createDocument(long token, String name, String mime) {
        forResult(token, new Intent(Intent.ACTION_CREATE_DOCUMENT).setType(mime).addCategory(Intent.CATEGORY_OPENABLE)
            .putExtra(Intent.EXTRA_TITLE, name), (code, data) -> {
            if (code != Activity.RESULT_OK || data == null || data.getData() == null) { Zgui.cancelled(token); return; }
            Uri uri = data.getData();
            Zgui.ok(token, uri.toString(), Integer.toString(activity.getContentResolver().openFileDescriptor(uri, "w").detachFd()));
        });
    }

    private void camera(long token) {
        File dir = ZguiFiles.dir(activity);
        dir.mkdirs();
        String name = "photo-" + System.currentTimeMillis() + ".jpg";
        Uri uri = ZguiFiles.uri(activity, name);
        Intent capture = new Intent(android.provider.MediaStore.ACTION_IMAGE_CAPTURE)
            .putExtra(android.provider.MediaStore.EXTRA_OUTPUT, uri)
            .addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_READ_URI_PERMISSION);
        capture.setClipData(ClipData.newRawUri("", uri));
        forResult(token, capture, (code, data) -> {
            File photo = new File(dir, name);
            if (code == Activity.RESULT_OK && photo.length() > 0) Zgui.ok(token, photo.getAbsolutePath());
            else { photo.delete(); Zgui.cancelled(token); }
        });
    }

    private void biometric(long token, String title, String subtitle, String cancel) {
        if (Build.VERSION.SDK_INT < 28) throw new UnsupportedOperationException("Biometric prompt needs Android 9");
        android.os.CancellationSignal signal = new android.os.CancellationSignal();
        android.hardware.biometrics.BiometricPrompt.Builder builder =
            new android.hardware.biometrics.BiometricPrompt.Builder(activity).setTitle(title);
        if (!subtitle.isEmpty()) builder.setSubtitle(subtitle);
        builder.setNegativeButton(cancel, activity.getMainExecutor(), (dialog, which) -> Zgui.cancelled(token))
            .build().authenticate(signal, activity.getMainExecutor(),
                new android.hardware.biometrics.BiometricPrompt.AuthenticationCallback() {
                    @Override public void onAuthenticationSucceeded(android.hardware.biometrics.BiometricPrompt.AuthenticationResult result) {
                        Zgui.ok(token);
                    }
                    @Override public void onAuthenticationError(int code, CharSequence message) {
                        if (code == android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_CANCELED
                                || code == android.hardware.biometrics.BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED)
                            Zgui.cancelled(token);
                        else Zgui.fail(token, new SecurityException(String.valueOf(message)));
                    }
                });
        Zgui.active.put(token, signal::cancel);
    }

    /** Token of the active NFC reader, or zero. Reader mode runs only while visible. */
    private long nfcToken;

    private void nfc(long token) {
        android.nfc.NfcAdapter adapter = android.nfc.NfcAdapter.getDefaultAdapter(activity);
        if (adapter == null || !adapter.isEnabled()) throw new IllegalStateException("NFC absent or disabled");
        nfcToken = token;
        readNfc(adapter);
        Zgui.active.put(token, () -> {
            if (nfcToken != token) return;
            nfcToken = 0;
            adapter.disableReaderMode(activity);
        });
    }

    private void readNfc(android.nfc.NfcAdapter adapter) {
        long token = nfcToken;
        adapter.enableReaderMode(activity, tag -> {
            android.nfc.tech.Ndef ndef = android.nfc.tech.Ndef.get(tag);
            android.nfc.NdefMessage message = ndef == null ? null : ndef.getCachedNdefMessage();
            String[] techs = tag.getTechList();
            String[] text = new String[techs.length + 1];
            StringBuilder id = new StringBuilder();
            for (byte b : tag.getId()) id.append(String.format("%02x", b));
            text[0] = id.toString();
            System.arraycopy(techs, 0, text, 1, techs.length);
            Zgui.send(token, false, Zgui.OK, text, message == null ? null : message.toByteArray());
        }, android.nfc.NfcAdapter.FLAG_READER_NFC_A | android.nfc.NfcAdapter.FLAG_READER_NFC_B
            | android.nfc.NfcAdapter.FLAG_READER_NFC_F | android.nfc.NfcAdapter.FLAG_READER_NFC_V, null);
    }

    /** Android 14+ progress preview. Cancellation restores view without dispatching Back. */
    private final class BackAnimation implements android.window.OnBackAnimationCallback {
        private void restore() {
            View decor = activity.getWindow().getDecorView();
            decor.setTranslationX(0);
            decor.setAlpha(1);
        }
        @Override public void onBackStarted(android.window.BackEvent event) { restore(); }
        @Override public void onBackProgressed(android.window.BackEvent event) {
            View decor = activity.getWindow().getDecorView();
            float direction = event.getSwipeEdge() == android.window.BackEvent.EDGE_LEFT ? 1 : -1;
            decor.setTranslationX(direction * event.getProgress() * 32 * activity.getResources().getDisplayMetrics().density);
            decor.setAlpha(1 - 0.15f * event.getProgress());
        }
        @Override public void onBackCancelled() { restore(); }
        @Override public void onBackInvoked() { restore(); back(); }
    }

    private void attach(String fontPath) {
        if (Build.VERSION.SDK_INT >= 33) {
            backCallback = Build.VERSION.SDK_INT >= 34 ? new BackAnimation() : ZguiHost::back;
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, backCallback);
        }
        editor = new HostEditor();
        editor.setBackgroundColor(Color.TRANSPARENT);
        editor.setTextColor(Color.TRANSPARENT);
        editor.setHighlightColor(Color.TRANSPARENT);
        editor.setCursorVisible(false);
        editor.setPadding(0, 0, 0, 0);
        editor.setIncludeFontPadding(false);
        editor.setSingleLine(true);
        editor.setGravity(Gravity.CENTER_VERTICAL | Gravity.LEFT);
        if (fontPath != null) editor.setTypeface(Typeface.createFromFile(fontPath));
        editor.setVisibility(View.GONE);
        editor.addTextChangedListener(new TextWatcher() {
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}
            public void onTextChanged(CharSequence s, int start, int before, int count) { report(); }
            public void afterTextChanged(Editable text) {}
        });
        activity.addContentView(editor, new FrameLayout.LayoutParams(1, 1));
        activity.getApplication().registerActivityLifecycleCallbacks(this);
        edgeToEdge();
        fastestRefresh();
        activity.registerComponentCallbacks(this);
        onConfigurationChanged(activity.getResources().getConfiguration());
    }

    @Override public void onConfigurationChanged(Configuration config) {
        boolean dark = (config.uiMode & Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES;
        systemBarIcons(barTheme == 0 ? dark : barTheme == 2);
        configurationChanged(dark, config.fontScale,
            config.getLayoutDirection() == View.LAYOUT_DIRECTION_RTL);
    }

    /** Sets the theme the application shows: 0 follows the system, 1 light, 2 dark. */
    public void barTheme(int theme) {
        main.post(() -> {
            barTheme = theme;
            onConfigurationChanged(activity.getResources().getConfiguration());
        });
    }

    /** Light icons over a dark theme; dark icons over a light one. */
    @SuppressWarnings("deprecation")
    private void systemBarIcons(boolean dark) {
        Window window = activity.getWindow();
        if (Build.VERSION.SDK_INT >= 30) {
            int light = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                | android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            window.getInsetsController().setSystemBarsAppearance(dark ? 0 : light, light);
        } else {
            View decor = window.getDecorView();
            int flags = decor.getSystemUiVisibility();
            int light = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                | (Build.VERSION.SDK_INT >= 26 ? View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR : 0);
            decor.setSystemUiVisibility(dark ? flags & ~light : flags | light);
        }
    }
    @SuppressWarnings("deprecation")
    @Override public void onLowMemory() {}

    /** kind: 0 tick, 1 long press, 2 confirm. Follows the system haptics setting. */
    public void haptic(int kind) {
        main.post(() -> {
            int feedback = kind == 1 ? HapticFeedbackConstants.LONG_PRESS
                : kind == 2 ? (Build.VERSION.SDK_INT >= 30 ? HapticFeedbackConstants.CONFIRM
                                                          : HapticFeedbackConstants.KEYBOARD_TAP)
                : HapticFeedbackConstants.CLOCK_TICK;
            activity.getWindow().getDecorView().performHapticFeedback(feedback);
        });
    }

    /** Opens Android's share sheet with plain text. */
    public void share(String text) {
        main.post(() -> {
            Intent send = new Intent(Intent.ACTION_SEND).setType("text/plain")
                .putExtra(Intent.EXTRA_TEXT, text);
            activity.startActivity(Intent.createChooser(send, null));
        });
    }

    /** Opens a link in the application that handles it. */
    public void openUrl(String url) {
        main.post(() -> {
            try {
                activity.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            } catch (ActivityNotFoundException ignored) {}
        });
    }

    /** Starts or stops tilt reports from the gravity sensor. */
    public void tilt(boolean on) {
        main.post(() -> {
            tilting = on;
            askedSensors = false;
            listenToTilt(on);
        });
    }

    private void listenToTilt(boolean on) {
        SensorManager sensors = (SensorManager)service(Context.SENSOR_SERVICE);
        sensors.unregisterListener(this);
        if (!on) return;
        if (sensorsDenied()) {
            // Ask once; the activity resumes after the dialog and listens if granted.
            if (!askedSensors) {
                askedSensors = true;
                activity.requestPermissions(new String[] {SENSOR_PERMISSION}, 2);
            }
            return;
        }
        Sensor gravity = sensors.getDefaultSensor(Sensor.TYPE_GRAVITY);
        if (gravity == null) gravity = sensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
        if (gravity != null) sensors.registerListener(this, gravity, SensorManager.SENSOR_DELAY_GAME);
    }

    /** Whether this system defines the sensor permission and has not granted it. */
    private boolean sensorsDenied() {
        try {
            activity.getPackageManager().getPermissionInfo(SENSOR_PERMISSION, 0);
        } catch (PackageManager.NameNotFoundException stock) {
            return false;
        }
        return activity.checkSelfPermission(SENSOR_PERMISSION) != PackageManager.PERMISSION_GRANTED;
    }

    @Override public void onSensorChanged(SensorEvent event) {
        // Device x points right and y up; report as screen directions in units of g.
        tilted(-event.values[0] / SensorManager.GRAVITY_EARTH, event.values[1] / SensorManager.GRAVITY_EARTH);
    }
    @Override public void onAccuracyChanged(Sensor sensor, int accuracy) {}

    /** Opens the system photo picker. Reports a PNG path in the cache, or null if cancelled. */
    @SuppressWarnings("deprecation")
    public void pickImage() {
        main.post(() -> {
            android.app.FragmentManager fragments = activity.getFragmentManager();
            PickFragment fragment = new PickFragment();
            fragment.host = this;
            fragments.beginTransaction().add(fragment, "zgui-pick").commitNow();
            Intent intent = Build.VERSION.SDK_INT >= 33
                ? new Intent("android.provider.action.PICK_IMAGES")
                : new Intent(Intent.ACTION_GET_CONTENT).setType("image/*");
            try {
                fragment.startActivityForResult(intent, 1);
            } catch (ActivityNotFoundException error) {
                fragments.beginTransaction().remove(fragment).commitNow();
                picked(null);
            }
        });
    }

    /** Receives the picker result. NativeActivity has no onActivityResult of its own. */
    @SuppressWarnings("deprecation")
    public static final class PickFragment extends android.app.Fragment {
        ZguiHost host;
        @Override public void onActivityResult(int request, int result, Intent data) {
            getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
            Uri uri = result == Activity.RESULT_OK && data != null ? data.getData() : null;
            if (host == null) return;
            if (uri == null) { picked(null); return; }
            ZguiHost owner = host;
            new Thread(() -> picked(owner.copyImage(uri))).start();
        }
    }

    /** Decodes, scales to at most 1600 pixels, and stores as PNG; null on failure. */
    private String copyImage(Uri uri) {
        try {
            BitmapFactory.Options bounds = new BitmapFactory.Options();
            bounds.inJustDecodeBounds = true;
            try (InputStream in = activity.getContentResolver().openInputStream(uri)) {
                BitmapFactory.decodeStream(in, null, bounds);
            }
            BitmapFactory.Options options = new BitmapFactory.Options();
            options.inSampleSize = 1;
            while (Math.max(bounds.outWidth, bounds.outHeight) / options.inSampleSize > 1600)
                options.inSampleSize *= 2;
            Bitmap bitmap;
            try (InputStream in = activity.getContentResolver().openInputStream(uri)) {
                bitmap = BitmapFactory.decodeStream(in, null, options);
            }
            if (bitmap == null) return null;
            File file = new File(activity.getCacheDir(), "zgui-picked-" + (++pickedImages) + ".png");
            try (FileOutputStream out = new FileOutputStream(file)) {
                bitmap.compress(Bitmap.CompressFormat.PNG, 100, out);
            }
            return file.getAbsolutePath();
        } catch (Exception error) {
            return null;
        }
    }

    /** Draws behind system bars and reports what they cover. */
    @SuppressWarnings("deprecation")
    private void edgeToEdge() {
        Window window = activity.getWindow();
        View decor = window.getDecorView();
        window.setStatusBarColor(Color.TRANSPARENT);
        window.setNavigationBarColor(Color.TRANSPARENT);
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false);
        } else {
            decor.setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams attributes = window.getAttributes();
            attributes.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(attributes);
        }
        decor.setOnApplyWindowInsetsListener((view, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets bars = insets.getInsets(
                    WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                android.graphics.Insets ime = insets.getInsets(WindowInsets.Type.ime());
                insetsChanged(bars.top, bars.right, bars.bottom, bars.left, ime.bottom);
            } else {
                insetsChanged(insets.getStableInsetTop(), insets.getStableInsetRight(),
                    insets.getStableInsetBottom(), insets.getStableInsetLeft(),
                    Math.max(0, insets.getSystemWindowInsetBottom() - insets.getStableInsetBottom()));
            }
            return view.onApplyWindowInsets(insets);
        });
        decor.requestApplyInsets();
    }

    /** Asks for the display's fastest mode at the current resolution. User settings can cap it,
     * so the rate in effect is reported now and on each display change. */
    @SuppressWarnings("deprecation")
    private void fastestRefresh() {
        Display display = activity.getWindowManager().getDefaultDisplay();
        ((DisplayManager)service(Context.DISPLAY_SERVICE)).registerDisplayListener(this, main);
        Display.Mode current = display.getMode(), fastest = current;
        for (Display.Mode mode : display.getSupportedModes()) {
            if (mode.getPhysicalWidth() == current.getPhysicalWidth()
                    && mode.getPhysicalHeight() == current.getPhysicalHeight()
                    && mode.getRefreshRate() > fastest.getRefreshRate()) fastest = mode;
        }
        WindowManager.LayoutParams attributes = activity.getWindow().getAttributes();
        attributes.preferredDisplayModeId = fastest.getModeId();
        activity.getWindow().setAttributes(attributes);
        refreshRate(display.getRefreshRate());
    }

    @SuppressWarnings("deprecation")
    @Override public void onDisplayChanged(int id) {
        Display display = activity.getWindowManager().getDefaultDisplay();
        if (display.getDisplayId() == id) refreshRate(display.getRefreshRate());
    }
    @Override public void onDisplayAdded(int id) {}
    @Override public void onDisplayRemoved(int id) {}

    /** Leaves the application as Android's Back does for a root activity. */
    public void goBack() {
        main.post(() -> activity.moveTaskToBack(true));
    }

    private final class HostEditor extends EditText {
        HostEditor() { super(activity); }
        @Override protected void onSelectionChanged(int start, int end) {
            super.onSelectionChanged(start, end);
            report();
        }
    }

    private void report() {
        if (syncing || target == 0 || editor == null) return;
        revision++;
        if (queued) return;
        queued = true;
        editor.post(() -> {
            queued = false;
            if (syncing || target == 0) return;
            changed(surface, target, revision, editor.getText().toString(),
                    editor.getSelectionStart(), editor.getSelectionEnd());
        });
    }

    private Object service(String name) { return activity.getSystemService(name); }

    // Called on native render thread; all View access stays on Android main thread.
    public void update(long surfaceId, long field, long acknowledged, String text,
                       int anchor, int focus, float x, float y, float width, float height,
                       float fontSize, boolean secret, boolean multiline) {
        main.post(() -> {
            if (editor == null) return;
            boolean newField = target != field;
            if (!newField && acknowledged < revision) return;
            syncing = true;
            try {
                surface = surfaceId;
                target = field;
                if (newField) revision = 0;
                if (field == 0) {
                    editor.clearFocus();
                    editor.setVisibility(View.GONE);
                    ((android.view.inputmethod.InputMethodManager)service(Context.INPUT_METHOD_SERVICE))
                        .hideSoftInputFromWindow(editor.getWindowToken(), 0);
                    return;
                }
                if (newField) {
                    editor.setInputType(InputType.TYPE_CLASS_TEXT |
                        (secret ? InputType.TYPE_TEXT_VARIATION_PASSWORD : InputType.TYPE_TEXT_FLAG_AUTO_CORRECT));
                    if (multiline) editor.setInputType(editor.getInputType() | InputType.TYPE_TEXT_FLAG_MULTI_LINE);
                    editor.setSingleLine(!multiline);
                    editor.setGravity((multiline ? Gravity.TOP : Gravity.CENTER_VERTICAL) | Gravity.LEFT);
                    editor.setFilters(multiline ? new InputFilter[0] : new InputFilter[] {
                        (source, start, end, dest, dstart, dend) -> {
                            String value = source.subSequence(start, end).toString();
                            return value.indexOf('\n') < 0 && value.indexOf('\r') < 0 ? null : value.replace('\n', ' ').replace('\r', ' ');
                        }
                    });
                    editor.setTextColor(Color.TRANSPARENT);
                    editor.setCursorVisible(false);
                }
                FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                    Math.max(1, Math.round(width)), Math.max(1, Math.round(height)));
                params.leftMargin = Math.round(x);
                params.topMargin = Math.round(y);
                editor.setLayoutParams(params);
                editor.setTextSize(TypedValue.COMPLEX_UNIT_PX, fontSize);
                if (!editor.getText().toString().equals(text)) editor.setText(text);
                int len = editor.length();
                int start = Math.min(Math.max(anchor, 0), len), end = Math.min(Math.max(focus, 0), len);
                if (editor.getSelectionStart() != start || editor.getSelectionEnd() != end)
                    editor.setSelection(start, end);
                editor.setVisibility(View.VISIBLE);
                editor.requestFocus();
                if (newField) showKeyboard();
            } finally { syncing = false; }
        });
    }

    public void touch(int action, float x, float y) {
        main.post(() -> {
            if (editor == null) return;
            long now = SystemClock.uptimeMillis();
            if (action == MotionEvent.ACTION_DOWN) touchDown = now;
            MotionEvent event = MotionEvent.obtain(touchDown, now, action, x, y, 0);
            event.setSource(InputDevice.SOURCE_TOUCHSCREEN);
            editor.dispatchTouchEvent(event);
            event.recycle();
        });
    }

    public void showKeyboard() {
        main.post(() -> {
            if (target != 0 && editor != null) {
                editor.requestFocus();
                ((android.view.inputmethod.InputMethodManager)service(Context.INPUT_METHOD_SERVICE))
                    .showSoftInput(editor, 0);
            }
        });
    }

    public String clipboard() {
        ClipData clip = ((ClipboardManager)service(Context.CLIPBOARD_SERVICE)).getPrimaryClip();
        return clip == null || clip.getItemCount() == 0 ? "" : clip.getItemAt(0).coerceToText(activity).toString();
    }

    public void setClipboard(String text, boolean secret) {
        ClipData clip = ClipData.newPlainText("", text);
        if (secret) {
            PersistableBundle extras = new PersistableBundle();
            extras.putBoolean("android.content.extra.IS_SENSITIVE", true);
            clip.getDescription().setExtras(extras);
        }
        ((ClipboardManager)service(Context.CLIPBOARD_SERVICE)).setPrimaryClip(clip);
    }

    /** Posts a system notification after delayMs. Tapping it returns to this activity. */
    public void notify(String title, String text, long delayMs) {
        main.postDelayed(() -> post(title, text), Math.max(0, delayMs));
    }

    private void post(String title, String text) {
        if (Build.VERSION.SDK_INT >= 33
                && activity.checkSelfPermission(NOTIFY_PERMISSION) != PackageManager.PERMISSION_GRANTED) {
            // Post when activity resumes after Android 13+ grants the runtime permission.
            pendingNotification = () -> post(title, text);
            activity.requestPermissions(new String[] {NOTIFY_PERMISSION}, 1);
            return;
        }
        NotificationManager manager = (NotificationManager)service(Context.NOTIFICATION_SERVICE);
        manager.createNotificationChannel(new NotificationChannel(
            CHANNEL, "Notifications", NotificationManager.IMPORTANCE_HIGH));
        PendingIntent open = PendingIntent.getActivity(activity, 0,
            activity.getPackageManager().getLaunchIntentForPackage(activity.getPackageName()),
            PendingIntent.FLAG_IMMUTABLE);
        // Status bar icons are drawn as a silhouette, so a full-colour app icon would be a white
        // square there; it goes beside the text as the large icon instead.
        int appIcon = activity.getApplicationInfo().icon;
        Notification.Builder builder = new Notification.Builder(activity, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info);
        if (appIcon != 0) builder.setLargeIcon(android.graphics.drawable.Icon.createWithResource(activity, appIcon));
        Notification notification = builder
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(new Notification.BigTextStyle().bigText(text))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build();
        manager.notify(++notifications, notification);
    }

    @Override public void onActivityResumed(Activity resumed) {
        if (resumed == activity) {
            ZguiDevice.visible(activity, true);
            android.nfc.NfcAdapter adapter = nfcToken == 0 ? null : android.nfc.NfcAdapter.getDefaultAdapter(activity);
            if (adapter != null) readNfc(adapter);
        }
        if (resumed == activity && tilting) listenToTilt(true);
        Runnable pending = pendingNotification;
        if (resumed != activity || pending == null) return;
        pendingNotification = null;
        if (activity.checkSelfPermission(NOTIFY_PERMISSION) == PackageManager.PERMISSION_GRANTED)
            pending.run();
    }

    // Android 10+. NativeActivity.onDestroy then waits for the native loop, so end it first.
    @Override public void onActivityPreDestroyed(Activity destroying) {
        if (destroying == activity) destroyed();
    }

    @Override public void onActivityDestroyed(Activity destroyed) {
        if (destroyed != activity) return;
        if (Zgui.host == this) Zgui.host = null;
        if (Build.VERSION.SDK_INT >= 33 && backCallback != null)
            activity.getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback(backCallback);
        activity.getApplication().unregisterActivityLifecycleCallbacks(this);
        ((DisplayManager)service(Context.DISPLAY_SERVICE)).unregisterDisplayListener(this);
    }

    @Override public void onActivityCreated(Activity a, Bundle state) {}
    @Override public void onActivityStarted(Activity a) {}
    @Override public void onActivityPaused(Activity paused) {
        if (paused == activity) {
            ZguiDevice.visible(activity, false);
            android.nfc.NfcAdapter adapter = nfcToken == 0 ? null : android.nfc.NfcAdapter.getDefaultAdapter(activity);
            if (adapter != null) adapter.disableReaderMode(activity);
        }
        if (paused == activity && tilting) listenToTilt(false);
    }
    @Override public void onActivityStopped(Activity a) {}
    @Override public void onActivitySaveInstanceState(Activity a, Bundle state) {}
}
