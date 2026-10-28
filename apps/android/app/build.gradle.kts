import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("io.github.takahirom.roborazzi")
}

// The version comes from the Rust workspace (Cargo.toml), like the desktop app's.
val release: String = Regex("""(?m)^version\s*=\s*"([^"]+)"""")
    .find(rootProject.file("../../Cargo.toml").readText())!!.groupValues[1]
// 1.4.0 -> 10400; a whole number that grows with each release.
val versionNumber = release.split(".").map { it.takeWhile(Char::isDigit).toInt() }.let { (a, b, c) -> a * 10_000 + b * 100 + c }

// The Dropbox app key, from the engine's sign-in keys (Dropbox comes back to db-<key>://).
val dropboxKey: String = Regex("""(?m)^dropbox_key\s*=\s*"([^"]*)"""")
    .find(rootProject.file("../../crates/marrow-cloud/keys.toml").readText())?.groupValues?.get(1)
    ?.ifBlank { null } ?: System.getenv("MARROW_DROPBOX_KEY") ?: "none"

// Signing for release builds: apps/android/keystore.properties (not in git) or environment variables in CI.
val signing = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
fun secret(key: String, env: String): String? = signing.getProperty(key) ?: System.getenv(env)

android {
    namespace = "app.marrow"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.marrow.notes"
        minSdk = 26
        targetSdk = 36
        versionCode = versionNumber
        versionName = release
        vectorDrawables.useSupportLibrary = true
        manifestPlaceholders["dropboxScheme"] = "db-$dropboxKey"
        // Phones and tablets (64- and 32-bit ARM) and the emulator.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }

    signingConfigs {
        create("release") {
            val store = secret("storeFile", "MARROW_KEYSTORE")
            if (store != null) {
                storeFile = file(store)
                storePassword = secret("storePassword", "MARROW_KEYSTORE_PASSWORD")
                keyAlias = secret("keyAlias", "MARROW_KEY_ALIAS")
                keyPassword = secret("keyPassword", "MARROW_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Signed with the release key when it's set up; otherwise with the
            // debug key, so the APK still installs (for trying it out).
            signingConfig = signingConfigs.getByName(if (secret("storeFile", "MARROW_KEYSTORE") != null) "release" else "debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures { compose = true }

    // The engine's bindings, written by `apps/build-android.sh` (not in git).
    sourceSets["main"].java.srcDir("src/generated/kotlin")

    packaging {
        // Compressed in the APK: a smaller download (Play Store bundles do their own thing).
        jniLibs.useLegacyPackaging = true
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }
    testOptions {
        unitTests {
            isIncludeAndroidResources = true
            all {
                // Screenshot tests load the engine built for this computer (target/debug).
                it.systemProperty("jna.library.path", rootProject.file("../../target/debug").absolutePath)
                it.systemProperty("robolectric.pixelCopyRenderMode", "hardware")
                it.systemProperty("marrow.demo", rootProject.file("../demo-vault").absolutePath)
                it.systemProperty("marrow.shots", file("screenshots").absolutePath)
                it.maxHeapSize = "3g"
                // File names in the demo vault are in Bangla too.
                it.environment("LC_ALL", "C.UTF-8")
                it.environment("LANG", "C.UTF-8")
            }
        }
    }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}

roborazzi {
    outputDir.set(file("screenshots"))
}

dependencies {
    val compose = platform("androidx.compose:compose-bom:2025.12.01")
    implementation(compose)
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.activity:activity-compose:1.12.2")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")
    implementation("androidx.lifecycle:lifecycle-process:2.9.4")
    implementation("androidx.navigation:navigation-compose:2.9.6")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-text-google-fonts")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material3:material3-window-size-class")
    implementation("androidx.compose.material3:material3-adaptive-navigation-suite")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("io.coil-kt.coil3:coil-compose:3.3.0")
    // Signing in to cloud services: the browser (Custom Tabs), and Google's own sign-in.
    implementation("androidx.browser:browser:1.8.0")
    implementation("com.google.android.gms:play-services-auth:21.3.0")
    implementation("net.java.dev.jna:jna:5.18.1@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.16")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.52.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.52.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-junit-rule:1.52.0")
    testImplementation("net.java.dev.jna:jna:5.18.1")
}
