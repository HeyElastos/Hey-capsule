buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        // Kotlin 2.x: godot-lib + fragment-compose ship kotlin-stdlib 2.1
        // metadata that the 1.9 compiler can't read. Compose compiler now
        // comes from the matching Kotlin plugin (applied in :app).
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.1.21")
        classpath("org.jetbrains.kotlin:compose-compiler-gradle-plugin:2.1.21")
    }
}
