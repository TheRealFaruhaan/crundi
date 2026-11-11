plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The node itself is the Crundi app's homepool package
// (../../android/app/src/main/java/mv/obitech/crundi/homepool), compiled
// here as it is: one implementation for both apps. Only that package is
// taken; it is copied into build/ before each build because a source folder
// cannot be narrowed to one package.
val coreSrc = rootProject.file("../../android/app/src/main/java")
val coreOut = layout.buildDirectory.dir("generated/homepool-core")
val syncCore by tasks.registering(Sync::class) {
    from(coreSrc) { include("mv/obitech/crundi/homepool/**") }
    into(coreOut)
}

android {
    namespace = "mv.obitech.homepoolnode"
    compileSdk = 36

    defaultConfig {
        applicationId = "mv.obitech.homepoolnode"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "2.0.0"
    }

    // Debug builds only need the SDK's debug key. A release build is signed
    // only when CI hands it the Homepool Node's OWN key (HOMEPOOL_NODE_*):
    // never Crundi's Play upload key, nor Crundi's GitHub APK key. No key is
    // kept in this repository; without one a release build comes out unsigned.
    val nodeKey = System.getenv("HOMEPOOL_NODE_STORE_FILE").orEmpty()
    val nodePassword = System.getenv("HOMEPOOL_NODE_STORE_PASSWORD").orEmpty()
    signingConfigs {
        if (nodeKey.isNotEmpty() && nodePassword.isNotEmpty()) {
            create("node") {
                storeFile = file(nodeKey)
                storePassword = nodePassword
                keyAlias = System.getenv("HOMEPOOL_NODE_KEY_ALIAS") ?: "homepool-node"
                keyPassword = nodePassword
            }
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("node")
        }
    }

    sourceSets.getByName("main") {
        java.srcDir(coreOut)
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

tasks.named("preBuild") { dependsOn(syncCore) }

dependencies {
    // What the homepool package needs (the Crundi app has the same).
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("androidx.core:core-ktx:1.17.0")
    // Scanning the node code (camera only while scanning).
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")

    testImplementation("junit:junit:4.13.2")
    // android.jar's org.json is stubs only; the real one for the JVM tests.
    testImplementation("org.json:json:20240303")
    // A stub Crundi server for the protocol test.
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
}
