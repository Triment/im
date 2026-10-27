plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val androidAbiTriples = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "arm-linux-androideabi",
    "x86_64" to "x86_64-linux-android",
    "x86" to "i686-linux-android",
)

val hostTag = when {
    System.getProperty("os.name").startsWith("Mac") -> "darwin-x86_64"
    System.getProperty("os.name").startsWith("Windows") -> "windows-x86_64"
    else -> "linux-x86_64"
}

val ndkRoot = providers.environmentVariable("ANDROID_NDK_HOME").orNull
    ?: providers.environmentVariable("ANDROID_NDK_ROOT").orNull
    ?: "${android.sdkDirectory}/ndk/${android.ndkVersion}"
val generatedJniLibs = layout.buildDirectory.dir("generated/jniLibs")

val copyAndroidCppRuntime = tasks.register<Copy>("copyAndroidCppRuntime") {
    androidAbiTriples.forEach { (abi, triple) ->
        from("$ndkRoot/toolchains/llvm/prebuilt/$hostTag/sysroot/usr/lib/$triple/libc++_shared.so") {
            into(abi)
        }
    }
    into(generatedJniLibs)
}

android {
    namespace = "com.example.xmp_flutter"
    compileSdk = 37
    ndkVersion = "28.2.13676358"

    sourceSets.getByName("main").jniLibs.srcDir(generatedJniLibs.get().asFile)

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "com.example.xmp_flutter"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        val keystorePath = providers.environmentVariable("ANDROID_KEYSTORE_PATH").orNull
        if (keystorePath != null) {
            create("release-ci") {
                storeFile = file(keystorePath)
                storePassword = providers.environmentVariable("ANDROID_KEYSTORE_PASSWORD").orNull
                keyAlias = providers.environmentVariable("ANDROID_KEY_ALIAS").orNull
                keyPassword = providers.environmentVariable("ANDROID_KEY_PASSWORD").orNull
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (providers.environmentVariable("ANDROID_KEYSTORE_PATH").isPresent) {
                signingConfigs.getByName("release-ci")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }
}

tasks.named("preBuild").configure {
    dependsOn(copyAndroidCppRuntime)
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
