import groovy.json.JsonSlurper
import java.net.URI
import java.security.MessageDigest
import javax.inject.Inject

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
        versionCode = 56
        versionName = "1.1.3-dev.25"

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

// ─── "Hey Crundi" (Phase 6): the sherpa-onnx keyword spotter ───
// The engine (sherpa-onnx, Apache-2.0: its Kotlin classes and its JNI library
// with ONNX Runtime linked in) and the English keyword model are too big for
// the repository, so they are fetched from sherpa-onnx's GitHub releases,
// pinned by version and SHA-256, cached in Gradle's home and unpacked into
// build/wakeword. Only the int8 model (about 5 MB) and the library for
// -PwakeWordAbis (default arm64-v8a, about 24 MB) go into the app.
// The keywords (src/main/assets/kws/keywords.txt) are the model's BPE tokens
// for "HEY CRU" (the stream made from the owner's profile carries its own
// line; voice/WakeProfile.kt), made with sherpa-onnx's text2token:
//   python -c "import sherpa_onnx; print(sherpa_onnx.text2token(['HEY CRU'],
//     tokens='tokens.txt', tokens_type='bpe', bpe_model='bpe.model'))"
abstract class FetchWakeWord : DefaultTask() {
    @get:Input abstract val engineUrl: Property<String>
    @get:Input abstract val engineSha256: Property<String>
    @get:Input abstract val modelUrl: Property<String>
    @get:Input abstract val modelSha256: Property<String>
    @get:Input abstract val abis: ListProperty<String>
    @get:Internal abstract val cacheDir: Property<String>
    @get:OutputDirectory abstract val jniDir: DirectoryProperty
    @get:OutputDirectory abstract val assetsDir: DirectoryProperty
    @get:OutputFile abstract val classesJar: RegularFileProperty
    @get:Inject abstract val archives: ArchiveOperations
    @get:Inject abstract val fs: FileSystemOperations

    private fun sha256(f: File): String = MessageDigest.getInstance("SHA-256").let { md ->
        f.inputStream().use { inp -> val buf = ByteArray(1 shl 16); while (true) { val n = inp.read(buf); if (n < 0) break; md.update(buf, 0, n) } }
        md.digest().joinToString("") { "%02x".format(it) }
    }

    /** The pinned file, from the cache or downloaded, refused unless its SHA-256 matches. */
    private fun fetch(url: String, sha: String): File {
        val dir = File(cacheDir.get()).apply { mkdirs() }
        val f = File(dir, url.substringAfterLast('/'))
        if (f.isFile && sha256(f) == sha) return f
        logger.lifecycle("Hey Crundi: downloading $url")
        val part = File(dir, f.name + ".part")
        try {
            URI(url).toURL().openConnection().apply { connectTimeout = 30_000; readTimeout = 120_000 }.getInputStream().use { inp -> part.outputStream().use { inp.copyTo(it) } }
        } catch (e: Exception) {
            throw GradleException("Hey Crundi: could not download $url (${e.message}). The build needs it once; it is cached in ${dir.path} after that.", e)
        }
        val got = sha256(part)
        if (got != sha) { part.delete(); throw GradleException("Hey Crundi: $url has SHA-256 $got, expected $sha. Refusing it.") }
        part.renameTo(f)
        return f
    }

    @TaskAction fun run() {
        val aar = fetch(engineUrl.get(), engineSha256.get())
        val model = fetch(modelUrl.get(), modelSha256.get())
        fs.delete { delete(jniDir, assetsDir) }
        for (abi in abis.get()) fs.copy {
            from(archives.zipTree(aar)) { include("jni/$abi/*.so") }
            eachFile { path = "$abi/$name" }
            includeEmptyDirs = false
            into(jniDir)
        }
        fs.copy {
            from(archives.zipTree(aar)) { include("classes.jar") }
            into(classesJar.get().asFile.parentFile)
            rename { classesJar.get().asFile.name }
        }
        // Fixed names, so the app doesn't need to know the model's epoch and chunk sizes.
        val names = mapOf(
            "encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx" to "encoder.int8.onnx",
            "decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx" to "decoder.int8.onnx",
            "joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx" to "joiner.int8.onnx",
            "tokens.txt" to "tokens.txt",
        )
        fs.copy {
            from(archives.tarTree(archives.bzip2(model))) { include(names.keys.map { "*/$it" }) }
            eachFile { path = "kws/" + names.getValue(name) }
            includeEmptyDirs = false
            into(assetsDir)
        }
        for (abi in abis.get()) if (!jniDir.get().file("$abi/libsherpa-onnx-jni.so").asFile.isFile) throw GradleException("Hey Crundi: no library for $abi in the engine")
        for (n in names.values) if (!assetsDir.get().file("kws/$n").asFile.isFile) throw GradleException("Hey Cru: the model has no $n")
    }
}

val wakeWordOut = layout.buildDirectory.dir("wakeword")
val fetchWakeWord = tasks.register<FetchWakeWord>("fetchWakeWord") {
    engineUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-static-link-onnxruntime-2.0.1.aar"
    engineSha256 = "07e042d97f3a6bd5b7fe74d4eef0513dc01a714b16c15a903250095aba43a575"
    modelUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2"
    modelSha256 = "f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a"
    abis = providers.gradleProperty("wakeWordAbis").orElse("arm64-v8a").map { it.split(',').map(String::trim).filter(String::isNotEmpty) }
    cacheDir = File(gradle.gradleUserHomeDir, "caches/crundi-wakeword").path
    jniDir = wakeWordOut.map { it.dir("jniLibs") }
    assetsDir = wakeWordOut.map { it.dir("assets") }
    classesJar = wakeWordOut.map { it.file("libs/sherpa-onnx.jar") }
}

android.sourceSets.getByName("main") {
    jniLibs.srcDir(wakeWordOut.get().dir("jniLibs"))
    assets.srcDir(wakeWordOut.get().dir("assets"))
}
// Every variant's build starts after the fetch.
tasks.named("preBuild") { dependsOn(fetchWakeWord) }

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
    // "Hey Crundi": sherpa-onnx's Kotlin API, from the pinned release (fetchWakeWord above).
    implementation(files(wakeWordOut.map { it.file("libs/sherpa-onnx.jar") }).builtBy(fetchWakeWord))
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
