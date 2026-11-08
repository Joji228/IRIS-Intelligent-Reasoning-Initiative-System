pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // JitPack builds the WebRTC voice detector (docs/VOICE-PLAN.md); only its own group is taken from there.
        maven("https://jitpack.io") {
            content { includeGroup("com.github.gkonovalov.android-vad") }
        }
    }
}

rootProject.name = "IRIS"
include(":app")
