# JNI entry points are looked up by exact class and method names.
-keep class md.vibecodium.mobile.CoreBridge { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
-keepattributes *Annotation*
