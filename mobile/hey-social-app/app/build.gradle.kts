import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Release signing — keys live in a gitignored keystore.properties (or CI env),
// NEVER in the repo. Without it the release build signs with no release key
// rather than silently using the PUBLIC debug key (which would let anyone ship a
// malicious same-signer update of a wallet app).
val keystorePropsFile = rootProject.file("keystore.properties")
val keystoreProps = Properties().apply {
    if (keystorePropsFile.exists()) keystorePropsFile.inputStream().use { load(it) }
}

android {
    namespace = "os.elastos.hey.social"
    // godot-lib's AAR metadata requires compileSdk >= 35 (we have 36 installed).
    compileSdk = 36

    defaultConfig {
        applicationId = "os.elastos.hey.social"
        minSdk = 26
        targetSdk = 34
        versionCode = 3
        versionName = "1.0"
        // arm64-only: the slim custom Godot build is arm64, real devices are
        // arm64, and the x86_64 emulator on this host is broken anyway. This
        // also halves the native-lib payload. (Restore x86_64 + the Maven
        // godot dep together if an emulator build is ever needed.)
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    signingConfigs {
        if (keystoreProps.getProperty("storeFile") != null) {
            create("release") {
                storeFile = rootProject.file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
                // v1 (jar) + v2 (full-APK) + v3 (key rotation lineage).
                enableV1Signing = true
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        getByName("debug") {
            isMinifyEnabled = false
            // Hey Verse is always available in dev/debug builds.
            buildConfigField("boolean", "VERSE_ENABLED", "true")
        }
        getByName("release") {
            // Verse on by default (so dev/test release builds keep it); ship the
            // "coming soon" build with:  ./gradlew assembleRelease -PverseEnabled=false
            buildConfigField("boolean", "VERSE_ENABLED", (project.findProperty("verseEnabled") ?: "true").toString())
            // R8: shrink + obfuscate. Native crypto is in the .so; this hardens the
            // Kotlin/JNI surface and drops dead code/resources.
            isMinifyEnabled = true
            isShrinkResources = true
            isDebuggable = false
            // Keep FULL native debug symbols (extracted alongside; the shipped .so
            // stays stripped) so MTE/GWP-ASan/Scudo crash reports symbolicate.
            ndk { debugSymbolLevel = "FULL" }
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    buildFeatures { compose = true; buildConfig = true }
    // Compose compiler version now rides the Kotlin plugin (2.x) — no composeOptions.
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.09.03")
    implementation(composeBom)
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.6")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
    // Image loading from the local /ipfs gateway.
    implementation("io.coil-kt:coil-compose:2.7.0")
    // QR generate (friend link) + scan (follow by QR).
    implementation("com.google.zxing:core:3.5.3")
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
    // Hardware-backed biometric unlock (identity vault) — used in a later step.
    implementation("androidx.biometric:biometric:1.1.0")
    // Hey Verse: Godot 4.6 engine + Compose<->Fragment interop hosting it in
    // a tab; the game ships as assets/verse.pck.
    // TEMPORARILY back on the Maven engine while the slim custom build
    // (libs/godot-lib-slim.aar) is rebuilt — v1 crashed on device; suspects:
    // 4.6.0-tag lib vs 4.6.3-editor pck + fully-stripped physics. v2 builds
    // from the 4.6.3-stable tag with classic physics restored.
    implementation("org.godotengine:godot:4.6.3.stable")
    // implementation(files("libs/godot-lib-slim.aar"))
    implementation("androidx.fragment:fragment-compose:1.8.9")
    // CameraX — capture for video calls (Preview self-view + ImageAnalysis → H.264 encoder).
    val camerax = "1.3.4"
    implementation("androidx.camera:camera-core:$camerax")
    implementation("androidx.camera:camera-camera2:$camerax")
    implementation("androidx.camera:camera-lifecycle:$camerax")
    implementation("androidx.camera:camera-view:$camerax")
    // Debug-only memory-leak detector (leaked Activities/Fragments/objects);
    // never ships in release (debugImplementation).
    debugImplementation("com.squareup.leakcanary:leakcanary-android:2.14")
}
