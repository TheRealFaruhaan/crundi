plugins {
    id("com.android.application") version "8.9.1" apply false
    id("org.jetbrains.kotlin.android") version "2.1.20" apply false
    // Compose's compiler ships with Kotlin and must be the same version.
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.20" apply false
    // The API client's models (mv.obitech.crundi.api) are read with
    // kotlinx.serialization; its plugin ships with Kotlin and must match it.
    id("org.jetbrains.kotlin.plugin.serialization") version "2.1.20" apply false
}
