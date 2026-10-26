import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

// Firebase's description of this app, for notifications. It is read from
// google-services.json if that file is here, and the file is not in the repo
// (see .gitignore). Without it the app builds and runs with notifications
// switched off, which is what a build from a fresh clone gets.
@Suppress("UNCHECKED_CAST")
val firebase: Map<String, String> = run {
    val f = file("google-services.json")
    if (!f.exists()) return@run emptyMap()
    val root = JsonSlurper().parse(f) as Map<String, Any?>
    val info = root["project_info"] as Map<String, Any?>
    val client = (root["client"] as List<Map<String, Any?>>).first()
    mapOf(
        "PROJECT_ID" to info["project_id"].toString(),
        "SENDER_ID" to info["project_number"].toString(),
        "APP_ID" to ((client["client_info"] as Map<String, Any?>)["mobilesdk_app_id"]).toString(),
        "API_KEY" to ((client["api_key"] as List<Map<String, Any?>>).first()["current_key"]).toString(),
    )
}

android {
    namespace = "mv.obitech.crundi"
    compileSdk = 36

    defaultConfig {
        applicationId = "mv.obitech.crundi"
        minSdk = 26
        targetSdk = 36
        versionCode = 31
        versionName = "1.1.2"

        // The oldest server this app can work with. The server holds the
        // mirror of this (MIN_CLIENT in src/client-version.js) and each side
        // refuses the other below its own number. Raise it only for a real
        // break: the Play Store updates the app by itself and nobody updates
        // a server by itself, so "app newer than server" is the normal case.
        // THE RULE (src/client-version.js): a new feature checks the server's
        // capabilities, and a missing endpoint says "not supported by this
        // server" instead of breaking, so this stays where it is for minor
        // releases. 1.21.1-dev.12 was the last raise of that kind.
        buildConfigField("String", "MIN_SERVER", "\"1.21.1-dev.12\"")
        for (k in listOf("PROJECT_ID", "SENDER_ID", "APP_ID", "API_KEY")) {
            buildConfigField("String", "FIREBASE_$k", "\"${firebase[k] ?: ""}\"")
        }
    }

    // How the app was installed decides where it looks for updates, and it is
    // decided HERE, when the app is built, not remembered at run time. The
    // `play` build does not contain the code that asks GitHub: that code is
    // in src/github and is simply not compiled into it.
    flavorDimensions += "store"
    productFlavors {
        create("github") {
            dimension = "store"
            buildConfigField("String", "STORE", "\"github\"")
        }
        create("play") {
            dimension = "store"
            buildConfigField("String", "STORE", "\"play\"")
        }
    }

    // Release builds are signed, each flavour with its own key, and where a
    // key is and what opens it come from the environment, never from this
    // repo. The `play` build is signed with the upload key, the one Google
    // knows:
    //   CRUNDI_UPLOAD_STORE_FILE      the keystore file
    //   CRUNDI_UPLOAD_STORE_PASSWORD  its password (also the key's)
    //   CRUNDI_UPLOAD_KEY_ALIAS       the key's name in it (default: upload)
    // The `github` build, the APK attached to a GitHub release, is signed
    // with a key of its own (CRUNDI_GITHUB_STORE_FILE, _STORE_PASSWORD,
    // _KEY_ALIAS, default: github). It is a different key on purpose: this
    // one has to live in the repository's secrets for the release workflow to
    // use, and a leak there must not be a leak of the key Play trusts.
    // Without its key a release build still compiles, unsigned, and cannot be
    // installed or uploaded: a build signed by accident with nothing is the
    // one mistake this arrangement cannot make.
    signingConfigs {
        for ((name, env) in listOf("upload" to "UPLOAD", "github" to "GITHUB")) {
            val store = System.getenv("CRUNDI_${env}_STORE_FILE").orEmpty()
            val password = System.getenv("CRUNDI_${env}_STORE_PASSWORD").orEmpty()
            if (store.isEmpty() || password.isEmpty()) continue
            create(name) {
                storeFile = file(store)
                storePassword = password
                keyAlias = System.getenv("CRUNDI_${env}_KEY_ALIAS") ?: name
                keyPassword = password
            }
        }
    }

    buildTypes {
        release {
            // Shrunk and renamed by R8. The build puts the mapping that undoes
            // the renaming into the bundle, so the Play Console can show crash
            // reports with real names. See proguard-rules.pro for what must
            // not be renamed.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // No key here: a build type's key would win over the flavour's,
            // and the flavours (below) are what decide it.
        }
    }

    // Which key signs which flavour. Set here, after both the flavours and
    // the keys exist. A debug build is not touched by this: its build type
    // carries the debug key, and a build type's key wins.
    productFlavors.getByName("play").signingConfig = signingConfigs.findByName("upload")
    productFlavors.getByName("github").signingConfig = signingConfigs.findByName("github")

    buildFeatures {
        buildConfig = true
        compose = true
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    testOptions {
        unitTests.isReturnDefaultValues = true
        // Robolectric (screenshot tests) needs the app's resources and manifest.
        unitTests.isIncludeAndroidResources = true
        unitTests.all {
            it.maxHeapSize = "1g"
            // Screenshot tests always record (write the PNG), never compare:
            // there is no golden set to compare with, the images are for people.
            it.systemProperty("roborazzi.test.record", "true")
            it.systemProperty("robolectric.graphicsMode", "NATIVE")
            it.systemProperty("robolectric.pixelCopyRenderMode", "hardware")
        }
    }
}


// The screenshot tests write every screen to build/outputs/screens/. This
// copies them to where the team wants them, filed by area:
// "<area>-<screen>.png" goes to <area>/, "edge-<case>-<area>.png" to <area>/.
//   ./gradlew --no-daemon testPlayDebugUnitTest exportScreens -PscreensDir=/path/to/shared/screens
val exportScreens = tasks.register<Copy>("exportScreens") {
    from(layout.projectDirectory.dir("build/outputs/screens"))
    include("*.png")
    eachFile {
        val stem = name.removeSuffix(".png")
        val area = if (stem.startsWith("edge-")) stem.substringAfterLast('-') else stem.substringBefore('-')
        path = "$area/$name"
    }
    includeEmptyDirs = false
    // Run in one command with the tests, it must copy what they wrote.
    mustRunAfter(tasks.matching { it.name.startsWith("test") && it.name.endsWith("UnitTest") })
    into(providers.gradleProperty("screensDir").orElse(layout.buildDirectory.dir("screens-export").map { it.asFile.path }))
}

dependencies {
    // Scanning the pairing picture (ScanActivity): the camera through
    // CameraX, the Crundi code read by our Kotlin port of the server's reader
    // (pair/CrundiCode.kt), its fallback QR code by ZXing. All in both flavours.
    implementation("androidx.camera:camera-core:1.4.2")
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.zxing:core:3.5.3")
    // Firebase brings Fragment 1.1, which predates the ActivityResult APIs the
    // pairing and scan screens use (release lint refuses it). A current one wins.
    implementation("androidx.fragment:fragment-ktx:1.8.6")
    // Secret PINs kept behind the fingerprint (BiometricPins): BiometricPrompt with a Keystore key.
    implementation("androidx.biometric:biometric:1.1.0")
    implementation("androidx.core:core-ktx:1.17.0")

    // The native UI: Jetpack Compose, one BOM pins every Compose library.
    val composeBom = platform("androidx.compose:compose-bom:2025.05.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.navigation:navigation-compose:2.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    // The API client (package api): OkHttp for HTTP, the live event stream
    // (SSE) and the WebSocket; kotlinx.serialization for the typed models.
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    // Device emulation sets the page's width before its scripts run (Browser.kt setViewport).
    implementation("androidx.webkit:webkit:1.12.1")
    // Video and audio Claude shows in a chat play in place (ui/chats/MediaPlayer.kt).
    implementation("androidx.media3:media3-exoplayer:1.5.1")
    implementation("androidx.media3:media3-ui:1.5.1")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.1")
    implementation("com.google.firebase:firebase-messaging:24.1.0")
    // Draws a project's icon when it is a drawing (.svg): Android itself
    // reads pictures made of dots only, and many a favicon is not one.
    implementation("com.caverock:androidsvg-aar:1.4")
    testImplementation("junit:junit:4.13.2")
    // Screenshot tests on the JVM (no emulator here): Robolectric draws, Roborazzi saves.
    testImplementation(composeBom)
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("org.robolectric:robolectric:4.15.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.43.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.43.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-junit-rule:1.43.1")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testImplementation("org.json:json:20240303")
    // A local HTTP/WebSocket server for the API client's tests.
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
}
