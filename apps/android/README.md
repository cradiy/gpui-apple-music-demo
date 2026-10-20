# Romus demo for Android

## Build

Install GPUiForge, JDK 21, the Android SDK and NDK, and the Rust Android targets:

```bash
rustup target add aarch64-linux-android x86_64-linux-android
```

Set `ANDROID_HOME` and `ANDROID_NDK_HOME` to the installed SDK and NDK directories.
Run from this directory:

```bash
gpuiforge doctor
gpuiforge build android --abi arm64-v8a
```

The debug APK is generated at
`target/gpuiforge/android/app/build/outputs/apk/debug/app-debug.apk`.
Use `--abi x86_64` for an x86_64 emulator. Omitting `--abi` builds both configured
architectures.

To build, install, and launch on a connected device:

```bash
gpuiforge devices
gpuiforge run android --device <device-id>
```

`gpuiforge.json` is the host configuration. Generated Kotlin, Gradle, and manifest
files under `target/` are managed by GPUiForge; update the configuration and run
`gpuiforge sync android` instead of editing generated files.

## Desktop preview

From the repository root:

```bash
cargo run -p romus-android
```

This opens the mobile layout in a desktop window. It does not emulate Android
system UI, touch input, or platform services. The default `cargo run` still starts
the desktop player.

## References

- [GPUI Android hosting](https://github.com/cradiy/gpui/blob/ace68f8616ba958d429edea50b61420acba7a232/crates/gpui_android/docs/hosting.md)
- [GPUiForge usage](https://github.com/cradiy/gpuiforge/blob/main/docs/usage.md)
