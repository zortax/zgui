# AccessKit Android 0.7.5 local patch

Source: https://github.com/AccessKit/accesskit
Crates.io package: https://crates.io/crates/accesskit_android/0.7.5
Package source and existing copyright notices preserved.

`src/event.rs`: check AccessibilityManager.isEnabled before event delivery and clear/report
Java exceptions from requestSendAccessibilityEvent rather than panicking across JNI.
Pixel 9/API 37 reproduction: UI automation activates tree, disconnects, then changing tab
sends content-change events. Android throws IllegalStateException (Accessibility off), and
upstream unwrap aborts entire app. Guard handles disconnect; exception branch handles race.
