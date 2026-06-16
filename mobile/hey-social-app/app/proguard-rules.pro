# ── JNI boundary ─────────────────────────────────────────────────────────────
# The Rust .so resolves Java_os_elastos_hey_social_* by exact name; R8 must NOT
# rename or strip the classes that declare `external fun` / native methods, or
# System.loadLibrary calls will UnsatisfiedLinkError at runtime.
-keepclasseswithmembernames,includedescriptorclasses class os.elastos.hey.social.** {
    native <methods>;
}
-keep class os.elastos.hey.social.HeyApi { *; }
-keep class os.elastos.hey.social.HeyRuntime { *; }
-keep class os.elastos.hey.social.BeamApi { *; }

# Result/DTO classes marshalled to/from JSON across JNI — keep their members so
# field access by name keeps working after shrinking.
-keepclassmembers class os.elastos.hey.social.** {
    <fields>;
}

# ── Godot engine (Hey Verse tab) ─────────────────────────────────────────────
# Godot loads its own native lib and reaches Java via JNI/reflection.
-keep class org.godotengine.** { *; }
-dontwarn org.godotengine.**
# GDScript calls the Verse bridge's @UsedByGodot methods BY NAME (reflection);
# renaming/stripping them would silently kill Verse multiplayer in release.
-keep class os.elastos.hey.social.HeyVersePlugin { *; }

# ── AndroidX biometric / Keystore (vault + app lock) ─────────────────────────
-keep class androidx.biometric.** { *; }
-dontwarn javax.crypto.**

# ── Coroutines / Compose are handled by their bundled consumer rules ─────────
# (No extra rules needed; left here as a marker if a future strip breaks them.)
