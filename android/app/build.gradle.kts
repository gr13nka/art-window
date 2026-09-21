plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// The tag version comes from Cargo.toml via CI, passed as -PappVersion=X.Y.Z; versionCode must
// strictly increase across releases, and the signing config must stay the same so updates install
// over each other rather than requiring an uninstall.
val appVersion = providers.gradleProperty("appVersion").getOrElse("0.1.0")
val versionParts = appVersion.split(".")
require(versionParts.size == 3) { "appVersion must be major.minor.patch, got \"$appVersion\"" }
val (versionMajor, versionMinor, versionPatch) = versionParts.map {
    it.toIntOrNull() ?: error("appVersion must be three numeric parts, got \"$appVersion\"")
}

val keystorePath = providers.environmentVariable("ANDROID_KEYSTORE_PATH").orNull

android {
    namespace = "dev.artwindow"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.artwindow"
        minSdk = 30
        targetSdk = 35
        versionCode = versionMajor * 10000 + versionMinor * 100 + versionPatch
        versionName = appVersion
    }

    signingConfigs {
        create("release") {
            // Only populated when CI provides a keystore; a local assembleRelease is left unsigned.
            if (!keystorePath.isNullOrEmpty()) {
                storeFile = file(keystorePath)
                storePassword = providers.environmentVariable("ANDROID_KEYSTORE_PASSWORD").getOrElse("")
                keyAlias = providers.environmentVariable("ANDROID_KEY_ALIAS").getOrElse("")
                keyPassword = providers.environmentVariable("ANDROID_KEY_PASSWORD").getOrElse("")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (!keystorePath.isNullOrEmpty()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.kotlinx.coroutines.android)

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
}
