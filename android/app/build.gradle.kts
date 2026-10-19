import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
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
        versionCode = 14
        versionName = "1.0.2-dev.2"

        // The oldest server this app can work with. The server holds the
        // mirror of this (MIN_CLIENT in src/client-version.js) and each side
        // refuses the other below its own number. Raise it only for a real
        // break: the Play Store updates the app by itself and nobody updates
        // a server by itself, so "app newer than server" is the normal case.
        buildConfigField("String", "MIN_SERVER", "\"1.20.0-dev.1\"")
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
    }
}

// The reader for the pairing picture is the server's own file, not a copy of
// it kept here: one implementation, drawn by the server and read by the app.
val copyCrundiCode = tasks.register<Copy>("copyCrundiCode") {
    from(rootProject.file("../src/crundi-code.js"))
    into(layout.projectDirectory.dir("src/main/assets"))
}
tasks.named("preBuild") { dependsOn(copyCrundiCode) }

dependencies {
    implementation("androidx.webkit:webkit:1.12.1")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("com.google.firebase:firebase-messaging:24.1.0")
    // Draws a project's icon when it is a drawing (.svg): Android itself
    // reads pictures made of dots only, and many a favicon is not one.
    implementation("com.caverock:androidsvg-aar:1.4")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}
