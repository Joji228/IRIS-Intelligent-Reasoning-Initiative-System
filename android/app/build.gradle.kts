import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "dev.iris.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.iris.app"
        minSdk = 29
        targetSdk = 36
        // versionCode only ever goes up (Android refuses to install a lower one over the
        // app already on the phone); versionName is what the user sees.
        versionCode = 1
        versionName = "0.1.0"
    }

    // Release signing comes from keystore.properties next to this folder's settings file,
    // git-ignored along with the keystore:
    //   storeFile=../iris-release.keystore
    //   storePassword=...
    //   keyAlias=...
    //   keyPassword=...
    // Without it, release builds are signed with the local debug key.
    val keystoreProps = rootProject.file("keystore.properties").takeIf { it.exists() }?.let { file ->
        Properties().apply { file.inputStream().use { load(it) } }
    }
    signingConfigs {
        if (keystoreProps != null) {
            create("release") {
                storeFile = file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)

    // Foundation only (no Material): the look is I.R.I.S.'s own, and the APK stays small.
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.ui.tooling.preview)
    debugImplementation(libs.androidx.compose.ui.tooling)

    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.okhttp)
    // Scans the pairing QR with Google Play services' own scanner: no camera permission,
    // nothing bundled.
    implementation(libs.play.services.code.scanner)

    testImplementation(libs.junit)
}
