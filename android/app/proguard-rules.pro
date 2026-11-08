# kotlinx.serialization and OkHttp ship their own consumer rules.
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**

# Google's QR scanner (ML Kit code scanner, play-services-code-scanner): it finds its parts by
# name at run time, which the shrinker can't see. Without these the release build crashed the
# moment "Scan the pairing QR code" was pressed (the debug build, which isn't shrunk, did not).
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_code_scanner.** { *; }
-keep class com.google.firebase.components.** { *; }
-keep class * implements com.google.firebase.components.ComponentRegistrar { *; }
