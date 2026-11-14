package dev.zgui.android;

import android.app.NativeActivity;
import android.content.Intent;
import android.content.res.Configuration;
import android.os.Bundle;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

/** Activity for ZGUI applications. Name it in the manifest instead of NativeActivity.
 * Java view dispatch keeps input, so TalkBack touch exploration works; Rust receives input
 * through native calls. Incoming intents and window mode changes reach Rust as events.
 */
public class ZguiActivity extends NativeActivity {
    private int mouseButton = MotionEvent.BUTTON_PRIMARY;

    @Override protected void onCreate(Bundle state) {
        Zgui.init(this);
        super.onCreate(state);
        // winit must not also read a native input queue, or input would arrive twice.
        getWindow().takeInputQueue(null);
        Zgui.intent(getIntent());
    }

    @Override protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        Zgui.intent(intent);
    }

    private static int modifiers(int meta) {
        return ((meta & KeyEvent.META_SHIFT_ON) != 0 ? 1 : 0) | ((meta & KeyEvent.META_CTRL_ON) != 0 ? 2 : 0)
            | ((meta & KeyEvent.META_ALT_ON) != 0 ? 4 : 0) | ((meta & KeyEvent.META_META_ON) != 0 ? 8 : 0);
    }

    /** Character text, or "named:" and a W3C key name. */
    private static String keyText(KeyEvent event, boolean plain) {
        String named;
        switch (event.getKeyCode()) {
        case KeyEvent.KEYCODE_BACK: case KeyEvent.KEYCODE_BUTTON_B: named = "BrowserBack"; break;
        case KeyEvent.KEYCODE_ESCAPE: named = "Escape"; break;
        case KeyEvent.KEYCODE_TAB: named = "Tab"; break;
        case KeyEvent.KEYCODE_ENTER: case KeyEvent.KEYCODE_NUMPAD_ENTER: case KeyEvent.KEYCODE_DPAD_CENTER:
        case KeyEvent.KEYCODE_BUTTON_A: named = "Enter"; break;
        case KeyEvent.KEYCODE_DEL: named = "Backspace"; break;
        case KeyEvent.KEYCODE_FORWARD_DEL: named = "Delete"; break;
        case KeyEvent.KEYCODE_DPAD_UP: named = "ArrowUp"; break;
        case KeyEvent.KEYCODE_DPAD_DOWN: named = "ArrowDown"; break;
        case KeyEvent.KEYCODE_DPAD_LEFT: named = "ArrowLeft"; break;
        case KeyEvent.KEYCODE_DPAD_RIGHT: named = "ArrowRight"; break;
        case KeyEvent.KEYCODE_MOVE_HOME: named = "Home"; break;
        case KeyEvent.KEYCODE_MOVE_END: named = "End"; break;
        case KeyEvent.KEYCODE_PAGE_UP: named = "PageUp"; break;
        case KeyEvent.KEYCODE_PAGE_DOWN: named = "PageDown"; break;
        case KeyEvent.KEYCODE_SPACE: named = " "; break;
        default:
            int unicode = event.getUnicodeChar(plain ? 0 : event.getMetaState());
            if (unicode > 0 && Character.isValidCodePoint(unicode)) return new String(Character.toChars(unicode));
            return "named:" + KeyEvent.keyCodeToString(event.getKeyCode());
        }
        return "named:" + named;
    }

    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        // The IME handles edits first. Back must reach Rust before the activity finishes.
        if (event.getKeyCode() != KeyEvent.KEYCODE_BACK && super.dispatchKeyEvent(event)) return true;
        if (event.getKeyCode() == KeyEvent.KEYCODE_VOLUME_UP || event.getKeyCode() == KeyEvent.KEYCODE_VOLUME_DOWN) return false;
        if (event.getAction() != KeyEvent.ACTION_DOWN && event.getAction() != KeyEvent.ACTION_UP) return false;
        try {
            Zgui.key(event.getAction() == KeyEvent.ACTION_DOWN, keyText(event, false), keyText(event, true),
                event.getScanCode(), modifiers(event.getMetaState()), event.getRepeatCount() > 0);
            return true;
        } catch (UnsatisfiedLinkError starting) { return false; }
    }

    @Override public boolean dispatchTouchEvent(MotionEvent event) {
        int action = event.getActionMasked();
        float density = getResources().getDisplayMetrics().density;
        int first = (action == MotionEvent.ACTION_POINTER_DOWN || action == MotionEvent.ACTION_POINTER_UP) ? event.getActionIndex() : 0;
        int end = (action == MotionEvent.ACTION_MOVE || action == MotionEvent.ACTION_CANCEL) ? event.getPointerCount() : first + 1;
        if (action == MotionEvent.ACTION_POINTER_DOWN) action = MotionEvent.ACTION_DOWN;
        if (action == MotionEvent.ACTION_POINTER_UP) action = MotionEvent.ACTION_UP;
        try {
            for (int i = first; i < end; i++) {
                boolean mouse = event.getToolType(i) == MotionEvent.TOOL_TYPE_MOUSE;
                if (mouse && action == MotionEvent.ACTION_DOWN) mouseButton = event.getButtonState();
                Zgui.pointer(action, event.getPointerId(i), event.getX(i) / density, event.getY(i) / density,
                    event.getPressure(i), event.getToolType(i), modifiers(event.getMetaState()),
                    mouse ? mouseButton : MotionEvent.BUTTON_PRIMARY);
            }
            return true;
        } catch (UnsatisfiedLinkError starting) { return super.dispatchTouchEvent(event); }
    }

    @Override public boolean dispatchGenericMotionEvent(MotionEvent event) {
        // AccessKit installs a hover listener on the content view; it sees events first.
        if (super.dispatchGenericMotionEvent(event)) return true;
        try {
            float density = getResources().getDisplayMetrics().density;
            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_SCROLL) {
                Zgui.wheel(event.getX() / density, event.getY() / density, event.getAxisValue(MotionEvent.AXIS_HSCROLL),
                    event.getAxisValue(MotionEvent.AXIS_VSCROLL), modifiers(event.getMetaState()));
                return true;
            }
            if ((event.getSource() & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) {
                Zgui.event("gamepad", new String[] {Float.toString(event.getAxisValue(MotionEvent.AXIS_X)),
                    Float.toString(event.getAxisValue(MotionEvent.AXIS_Y)), Float.toString(event.getAxisValue(MotionEvent.AXIS_Z)),
                    Float.toString(event.getAxisValue(MotionEvent.AXIS_RZ))});
                return true;
            }
            if (action == MotionEvent.ACTION_HOVER_ENTER || action == MotionEvent.ACTION_HOVER_MOVE || action == MotionEvent.ACTION_HOVER_EXIT) {
                Zgui.pointer(action == MotionEvent.ACTION_HOVER_MOVE ? MotionEvent.ACTION_MOVE : action, 0,
                    event.getX() / density, event.getY() / density, event.getPressure(), event.getToolType(0),
                    modifiers(event.getMetaState()), 0);
                return true;
            }
        } catch (UnsatisfiedLinkError starting) { return false; }
        return false;
    }

    private void windowMode() {
        try {
            Zgui.event("window", new String[] {Zgui.bool(isInMultiWindowMode())[0], Zgui.bool(isInPictureInPictureMode())[0]});
        } catch (UnsatisfiedLinkError starting) { /* Rust reads the mode when it asks. */ }
    }
    @Override public void onMultiWindowModeChanged(boolean mode, Configuration config) {
        super.onMultiWindowModeChanged(mode, config);
        windowMode();
    }
    @Override public void onPictureInPictureModeChanged(boolean mode, Configuration config) {
        super.onPictureInPictureModeChanged(mode, config);
        windowMode();
    }
}
