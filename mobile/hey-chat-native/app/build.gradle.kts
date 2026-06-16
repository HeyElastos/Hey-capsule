plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "os.elastos.hey.chat"
    compileSdk = 34

    defaultConfig {
        applicationId = "os.elastos.hey.chat"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        // The Rust runtime is shipped as a prebuilt .so under src/main/jniLibs.
        // arm64 only for now (every modern phone); add x86_64 for emulators later.
        // arm64 = real phones; x86_64 = emulators / Waydroid.
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    buildTypes {
        getByName("debug") {
            isMinifyEnabled = false
        }
        getByName("release") {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    // The 4.3 MB WASM in assets/ must NOT be compressed, or the WebView streams
    // a corrupt body (and WebAssembly.instantiateStreaming fails).
    androidResources {
        noCompress += listOf("wasm")
    }
}

dependencies {
    // Hardware-backed biometric unlock for the identity vault (next layer).
    implementation("androidx.biometric:biometric:1.1.0")
}
