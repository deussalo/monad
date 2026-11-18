#!/bin/sh
# Builds the Android APK: Rust (cogs-app) to jniLibs with cargo-ndk, then gradle packages it.
set -e
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH" ANDROID_HOME=/srv/rig/android-sdk ANDROID_NDK_HOME=/srv/rig/android-sdk/ndk/26.1.10909125 JAVA_HOME=/srv/rig/jdk-17
cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs build -p cogs-app --release
cd android && ./gradlew --no-daemon -q -Dorg.gradle.java.home=$JAVA_HOME :app:assembleRelease
ls -la app/build/outputs/apk/release/app-release.apk
