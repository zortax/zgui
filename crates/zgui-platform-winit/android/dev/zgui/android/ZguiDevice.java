package dev.zgui.android;

import android.app.ActivityManager;
import android.app.PendingIntent;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothGatt;
import android.bluetooth.BluetoothGattCallback;
import android.bluetooth.BluetoothGattService;
import android.bluetooth.BluetoothManager;
import android.bluetooth.BluetoothProfile;
import android.bluetooth.le.BluetoothLeScanner;
import android.bluetooth.le.ScanCallback;
import android.bluetooth.le.ScanResult;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.hardware.Sensor;
import android.hardware.SensorManager;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbManager;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.media.MediaRecorder;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.os.BatteryManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Looper;
import android.os.ParcelFileDescriptor;
import android.os.PowerManager;
import android.os.SystemClock;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.view.InputDevice;
import java.security.KeyStore;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** Device services that need a context but no activity, so background processes can use them. */
final class ZguiDevice {
    private ZguiDevice() {}

    static String[] call(Context c, String op, long token, String[] a) throws Exception {
        switch (op) {
        case "location": location(c, token, a); return null;
        case "location.enabled": {
            LocationManager locations = c.getSystemService(LocationManager.class);
            boolean enabled = false;
            for (String provider : providers(a[0])) enabled |= locations.isProviderEnabled(provider);
            return Zgui.bool(enabled);
        }
        case "ble.scan": bleScan(c, token); return null;
        case "ble.connect": bleConnect(c, token, a[0]); return null;
        case "usb.list": return usbList(c);
        case "usb.permission": usbPermission(c, token, a[0]); return null;
        case "usb.open": return new String[] {Integer.toString(usbOpen(c, a[0]))};
        case "sensors": {
            List<String> out = new ArrayList<>();
            for (Sensor sensor : c.getSystemService(SensorManager.class).getSensorList(Sensor.TYPE_ALL))
                Collections.addAll(out, sensor.getName(), Integer.toString(sensor.getType()));
            return out.toArray(new String[0]);
        }
        case "input.devices": {
            List<String> out = new ArrayList<>();
            for (int id : InputDevice.getDeviceIds()) {
                InputDevice device = InputDevice.getDevice(id);
                if (device != null) Collections.addAll(out, Integer.toString(id), device.getName(), Integer.toString(device.getSources()));
            }
            return out.toArray(new String[0]);
        }
        case "power": return power(c);
        case "network": return network(c);
        case "wake": {
            PowerManager.WakeLock lock = c.getSystemService(PowerManager.class)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "zgui:" + token);
            lock.acquire(Long.parseLong(a[0]));
            Zgui.active.put(token, () -> { if (lock.isHeld()) lock.release(); });
            return null;
        }
        case "audio.record": Zgui.onMain(token, () -> record(token, a[0], Integer.parseInt(a[1]))); return null;
        case "audio.play": Zgui.onMain(token, () -> play(c, token, a[0])); return null;
        case "keystore.delete": {
            KeyStore store = KeyStore.getInstance("AndroidKeyStore");
            store.load(null);
            store.deleteEntry(a[0]);
            return null;
        }
        default: throw new IllegalArgumentException("Unknown operation " + op);
        }
    }

    static byte[] bytes(String op, String[] a, byte[] data) throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        String alias = a[0];
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        switch (op) {
        case "keystore.encrypt": {
            if (!store.containsAlias(alias)) {
                KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
                generator.init(new KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
                generator.generateKey();
            }
            cipher.init(Cipher.ENCRYPT_MODE, (SecretKey)store.getKey(alias, null));
            byte[] iv = cipher.getIV(), sealed = cipher.doFinal(data);
            byte[] out = new byte[iv.length + sealed.length];
            System.arraycopy(iv, 0, out, 0, iv.length);
            System.arraycopy(sealed, 0, out, iv.length, sealed.length);
            return out;
        }
        case "keystore.decrypt": {
            SecretKey key = (SecretKey)store.getKey(alias, null);
            if (key == null) throw new IllegalArgumentException("No key named " + alias);
            if (data.length < 12) throw new IllegalArgumentException("Ciphertext too short");
            cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(128, data, 0, 12));
            return cipher.doFinal(data, 12, data.length - 12);
        }
        default: throw new IllegalArgumentException("Unknown operation " + op);
        }
    }

    // Location

    private static String[] providers(String source) {
        if (source.equals("gps")) return new String[] {LocationManager.GPS_PROVIDER};
        if (source.equals("network")) return new String[] {LocationManager.NETWORK_PROVIDER};
        return new String[] {LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER};
    }

    @SuppressWarnings("deprecation")
    private static final class Watcher implements LocationListener {
        final long token, interval;
        final List<String> providers;
        Watcher(long token, List<String> providers, long interval) {
            this.token = token;
            this.providers = providers;
            this.interval = interval;
        }
        void start(LocationManager locations) {
            for (String provider : providers) {
                if (locations.getAllProviders().contains(provider))
                    locations.requestLocationUpdates(provider, interval, 0, this, Looper.getMainLooper());
            }
        }
        @Override public void onLocationChanged(Location l) {
            long age = (SystemClock.elapsedRealtimeNanos() - l.getElapsedRealtimeNanos()) / 1_000_000;
            Zgui.next(token, Double.toString(l.getLatitude()), Double.toString(l.getLongitude()),
                l.hasAccuracy() ? Float.toString(l.getAccuracy()) : "",
                l.hasAltitude() ? Double.toString(l.getAltitude()) : "",
                l.hasSpeed() ? Float.toString(l.getSpeed()) : "",
                l.hasBearing() ? Float.toString(l.getBearing()) : "",
                Long.toString(l.getTime()), Long.toString(age), Zgui.str(l.getProvider()));
        }
        @Override public void onProviderDisabled(String provider) { Zgui.next(token, "disabled", provider); }
        @Override public void onProviderEnabled(String provider) {}
        @Override public void onStatusChanged(String provider, int status, Bundle extras) {}
    }
    private static final Map<Long, Watcher> watchers = new ConcurrentHashMap<>();

    /** a: source (gps, network or any), interval ms, deliver last known ("1"). */
    private static void location(Context c, long token, String[] a) {
        boolean fine = c.checkSelfPermission("android.permission.ACCESS_FINE_LOCATION") == PackageManager.PERMISSION_GRANTED;
        boolean coarse = fine || c.checkSelfPermission("android.permission.ACCESS_COARSE_LOCATION") == PackageManager.PERMISSION_GRANTED;
        List<String> allowed = new ArrayList<>();
        for (String provider : providers(a[0])) {
            if (provider.equals(LocationManager.GPS_PROVIDER) ? fine : coarse) allowed.add(provider);
        }
        if (allowed.isEmpty()) throw new SecurityException("Location permission not granted");
        LocationManager locations = c.getSystemService(LocationManager.class);
        Watcher watcher = new Watcher(token, allowed, Long.parseLong(a[1]));
        watchers.put(token, watcher);
        Zgui.active.put(token, () -> {
            watchers.remove(token);
            locations.removeUpdates(watcher);
        });
        Zgui.onMain(token, () -> {
            if (!watchers.containsKey(token)) return;
            watcher.start(locations);
            if (!"1".equals(a[2])) return;
            for (String provider : allowed) {
                Location last = locations.getLastKnownLocation(provider);
                if (last != null) watcher.onLocationChanged(last);
            }
        });
    }

    /** Android allows background location only for foreground services. Watchers stop while the
     * activity is hidden unless this process runs `ZguiService`. Main thread only. */
    static void visible(Context c, boolean visible) {
        if (ZguiService.current != null || watchers.isEmpty()) return;
        LocationManager locations = c.getSystemService(LocationManager.class);
        for (Watcher watcher : watchers.values()) {
            try {
                if (visible) watcher.start(locations);
                else locations.removeUpdates(watcher);
            } catch (SecurityException revoked) {
                Zgui.active.remove(watcher.token);
                watchers.remove(watcher.token);
                Zgui.fail(watcher.token, revoked);
            }
        }
    }

    // Bluetooth

    private static BluetoothAdapter adapter(Context c) {
        BluetoothManager manager = c.getSystemService(BluetoothManager.class);
        BluetoothAdapter adapter = manager == null ? null : manager.getAdapter();
        if (adapter == null || !adapter.isEnabled()) throw new IllegalStateException("Bluetooth absent or disabled");
        return adapter;
    }

    private static void bleScan(Context c, long token) {
        BluetoothLeScanner scanner = adapter(c).getBluetoothLeScanner();
        if (scanner == null) throw new IllegalStateException("BLE scanner unavailable");
        ScanCallback callback = new ScanCallback() {
            @Override public void onScanResult(int type, ScanResult result) {
                String name = null;
                try { name = result.getDevice().getName(); } catch (SecurityException noConnectPermission) { /* Address only. */ }
                Zgui.next(token, result.getDevice().getAddress(), Zgui.str(name), Integer.toString(result.getRssi()));
            }
            @Override public void onScanFailed(int code) {
                Zgui.fail(token, new IllegalStateException("BLE scan failed: " + code));
            }
        };
        scanner.startScan(callback);
        Zgui.active.put(token, () -> {
            try { scanner.stopScan(callback); } catch (RuntimeException adapterOff) { /* Already stopped. */ }
        });
    }

    private static void bleConnect(Context c, long token, String address) {
        BluetoothGatt gatt = adapter(c).getRemoteDevice(address).connectGatt(c, false, new BluetoothGattCallback() {
            @Override public void onConnectionStateChange(BluetoothGatt client, int status, int state) {
                if (state == BluetoothProfile.STATE_CONNECTED) {
                    Zgui.next(token, "connected");
                    client.discoverServices();
                } else {
                    Zgui.next(token, "disconnected", Integer.toString(status));
                }
            }
            @Override public void onServicesDiscovered(BluetoothGatt client, int status) {
                List<String> out = new ArrayList<>();
                out.add("services");
                for (BluetoothGattService service : client.getServices()) out.add(service.getUuid().toString());
                Zgui.next(token, out.toArray(new String[0]));
            }
        });
        Zgui.active.put(token, () -> {
            gatt.disconnect();
            gatt.close();
        });
    }

    // USB

    private static UsbDevice usbDevice(UsbManager usb, String name) {
        UsbDevice device = usb.getDeviceList().get(name);
        if (device == null) throw new IllegalArgumentException("USB device not attached");
        return device;
    }

    private static String[] usbList(Context c) {
        UsbManager usb = c.getSystemService(UsbManager.class);
        List<String> out = new ArrayList<>();
        for (UsbDevice device : usb.getDeviceList().values()) {
            Collections.addAll(out, device.getDeviceName(), Integer.toString(device.getVendorId()),
                Integer.toString(device.getProductId()), Integer.toString(device.getInterfaceCount()),
                usb.hasPermission(device) ? "1" : "0");
        }
        return out.toArray(new String[0]);
    }

    private static void usbPermission(Context c, long token, String name) {
        UsbManager usb = c.getSystemService(UsbManager.class);
        UsbDevice device = usbDevice(usb, name);
        if (usb.hasPermission(device)) { Zgui.ok(token, "1"); return; }
        Intent result = new Intent(c, ZguiReceiver.class).setAction(ZguiReceiver.USB).putExtra("token", token);
        // The system adds the grant to this intent, so it must be mutable.
        usb.requestPermission(device, PendingIntent.getBroadcast(c, (int)token, result,
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_MUTABLE));
    }

    /** Returns a duplicate of the usbfs descriptor; the caller owns it. */
    private static int usbOpen(Context c, String name) throws Exception {
        UsbManager usb = c.getSystemService(UsbManager.class);
        UsbDeviceConnection connection = usb.openDevice(usbDevice(usb, name));
        if (connection == null) throw new SecurityException("USB access not granted");
        try {
            return ParcelFileDescriptor.fromFd(connection.getFileDescriptor()).detachFd();
        } finally {
            connection.close();
        }
    }

    // Status

    private static String[] power(Context c) {
        PowerManager power = c.getSystemService(PowerManager.class);
        BatteryManager battery = c.getSystemService(BatteryManager.class);
        boolean restricted = Build.VERSION.SDK_INT >= 28 && c.getSystemService(ActivityManager.class).isBackgroundRestricted();
        return new String[] {
            Zgui.bool(power.isIgnoringBatteryOptimizations(c.getPackageName()))[0],
            Zgui.bool(power.isPowerSaveMode())[0], Zgui.bool(power.isDeviceIdleMode())[0], Zgui.bool(restricted)[0],
            Integer.toString(battery.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)),
            Zgui.bool(battery.isCharging())[0]};
    }

    private static String[] network(Context c) {
        ConnectivityManager connectivity = c.getSystemService(ConnectivityManager.class);
        NetworkCapabilities caps = connectivity.getNetworkCapabilities(connectivity.getActiveNetwork());
        if (caps == null) return null;
        return new String[] {
            Zgui.bool(caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED))[0],
            Zgui.bool(!caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))[0],
            Zgui.bool(caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))[0],
            Zgui.bool(caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR))[0],
            Zgui.bool(caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET))[0],
            Zgui.bool(caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN))[0]};
    }

    // Audio

    @SuppressWarnings("deprecation")
    private static void record(long token, String path, int limitMs) throws Exception {
        MediaRecorder recorder = new MediaRecorder();
        recorder.setAudioSource(MediaRecorder.AudioSource.MIC);
        recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4);
        recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC);
        recorder.setOutputFile(path);
        if (limitMs > 0) recorder.setMaxDuration(limitMs);
        recorder.setOnInfoListener((r, what, extra) -> {
            if (what == MediaRecorder.MEDIA_RECORDER_INFO_MAX_DURATION_REACHED) end(token, Zgui.OK, null);
        });
        try {
            recorder.prepare();
            recorder.start();
        } catch (Exception error) {
            recorder.release();
            throw error;
        }
        Zgui.active.put(token, () -> {
            try { recorder.stop(); } catch (RuntimeException tooShort) { /* No audio captured. */ }
            recorder.release();
        });
    }

    private static void play(Context c, long token, String path) throws Exception {
        AudioManager audio = c.getSystemService(AudioManager.class);
        AudioAttributes attributes = new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build();
        AudioFocusRequest focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(attributes)
            .setOnAudioFocusChangeListener(change -> { if (change < 0) end(token, Zgui.CANCELLED, null); }).build();
        if (audio.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
            throw new IllegalStateException("Audio focus denied");
        MediaPlayer player = new MediaPlayer();
        MediaSession session = new MediaSession(c, "zgui");
        Zgui.active.put(token, () -> {
            player.release();
            session.release();
            audio.abandonAudioFocusRequest(focus);
        });
        player.setAudioAttributes(attributes);
        player.setDataSource(path);
        player.setOnCompletionListener(p -> end(token, Zgui.OK, null));
        player.setOnErrorListener((p, what, extra) -> { end(token, Zgui.ERROR, "Playback error " + what + "/" + extra); return true; });
        session.setCallback(new MediaSession.Callback() {
            @Override public void onPause() { end(token, Zgui.CANCELLED, null); }
            @Override public void onStop() { end(token, Zgui.CANCELLED, null); }
        });
        session.setPlaybackState(new PlaybackState.Builder()
            .setActions(PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_STOP)
            .setState(PlaybackState.STATE_PLAYING, 0, 1).build());
        session.setActive(true);
        player.setOnPreparedListener(MediaPlayer::start);
        player.prepareAsync();
    }

    /** Stops the operation of `token` and reports how it ended. */
    private static void end(long token, int code, String message) {
        Runnable stop = Zgui.active.remove(token);
        if (stop == null) return;
        stop.run();
        Zgui.send(token, true, code, message == null ? null : new String[] {message}, null);
    }
}
