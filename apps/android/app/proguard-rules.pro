# The engine is reached through JNA (UniFFI bindings): keep both as they are.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class app.marrow.core.** { *; }
-dontwarn java.awt.**
