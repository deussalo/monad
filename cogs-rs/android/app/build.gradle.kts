plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The Rust side (libcogs.so, from cogs-app) is built by cargo-ndk into src/main/jniLibs: see build.sh.
android {
    namespace = "help.techmate.cogs"
    compileSdk = 35

    defaultConfig {
        applicationId = "help.techmate.cogs"
        minSdk = 26
        targetSdk = 35
        versionCode = 15
        versionName = "0.4.11"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Signed with the debug key so the APK installs directly by sideloading.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
