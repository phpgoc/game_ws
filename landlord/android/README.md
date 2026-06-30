# Landlord Android WS Server

Android/Kotlin version of the Landlord websocket server.

## What It Does

- Starts a websocket server on port `9001`.
- Shows the current LAN address, port, connected clients, and room count.
- Runs inside a foreground service with a persistent notification.
- Uses `WakeLock` and `WifiLock` to reduce the chance of being suspended in the background.
- Implements the Landlord websocket routes used by the current web client:
  - `JOIN`
  - `QUIT`
  - `DISBAND`
  - `SETTING`
  - `START`
  - `CALL_LANDLORD`
  - `PLAY`
  - `AWAY`
  - `BACK`

## Build

Open `ws/landlord/android` in Android Studio, or run:

```bash
JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" gradle --no-daemon :app:assembleDebug
```

This workspace does not include a Gradle wrapper yet.

On this machine the Android SDK is configured in `local.properties`:

```properties
sdk.dir=/Users/yangdianqing/Library/Android/sdk
```

If you prefer the SDKMAN JDK, this also works:

```bash
JAVA_HOME="$HOME/.sdkman/candidates/java/current" gradle --no-daemon :app:assembleDebug
```

## Notes

- The server is intended for LAN / hotspot usage. Mobile networks usually put phones behind NAT, so other devices may not be able to connect.
- Android can still kill long-running background work under memory pressure, aggressive OEM battery policies, or if the user force-stops the app.
- The app asks for notification permission on Android 13+ and provides a button to open battery optimization settings.
- This is a Kotlin implementation, not a JNI wrapper around the Rust server.

## Before Shipping

- Test on target Android devices and ROMs.
- Decide whether to add boot auto-start.
- Add a Gradle wrapper if this project should be built outside Android Studio.
- Consider adding instrumentation tests for websocket flows.
