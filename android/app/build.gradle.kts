plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.crundi.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.crundi.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "1.0.0-dev.1"

        // The oldest server this app can work with. The server holds the
        // mirror of this (MIN_CLIENT in src/client-version.js) and each side
        // refuses the other below its own number. Raise it only for a real
        // break: the Play Store updates the app by itself and nobody updates
        // a server by itself, so "app newer than server" is the normal case.
        buildConfigField("String", "MIN_SERVER", "\"1.20.0-dev.1\"")
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

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
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
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}
