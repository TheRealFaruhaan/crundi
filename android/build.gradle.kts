plugins {
    id("com.android.application") version "8.9.1" apply false
    id("org.jetbrains.kotlin.android") version "2.1.20" apply false
    // Compose's compiler ships with Kotlin and must be the same version.
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.20" apply false
}
