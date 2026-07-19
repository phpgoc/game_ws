import org.gradle.api.GradleException
import org.gradle.api.tasks.Exec

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

data class GameBuildConfig(
    val applicationId: String,
    val port: Int,
)

val games = mapOf(
    "landlord" to GameBuildConfig("com.example.langame.landlordserver", 9001),
    "shenyang_mahjong" to GameBuildConfig("com.example.langame.shenyangmahjongserver", 9002),
    "holdem" to GameBuildConfig("com.example.langame.holdemserver", 9003),
    "tractor" to GameBuildConfig("com.example.langame.tractorserver", 9004),
    "p2p" to GameBuildConfig("com.example.langame.p2pserver", 9005),
)
val game = providers.gradleProperty("game").orElse("landlord").get()
val gameConfig = games[game]
    ?: throw GradleException("Unknown -Pgame=$game. Expected one of: ${games.keys.joinToString()}")
val skipRustBuild = providers.gradleProperty("skipRustBuild").orNull?.toBoolean() ?: false
val rustAbis = providers.gradleProperty("rustAbis")
    .orNull
    ?.split(",")
    ?.map { it.trim() }
    ?.filter { it.isNotEmpty() }
    ?: listOf("arm64-v8a", "x86_64")
val rustProjectDir = layout.projectDirectory.dir("../../rust/$game")
val rustCommonDir = layout.projectDirectory.dir("../../rust/common")
val rustShareTypesDir = layout.projectDirectory.dir("../../share_type_public")
val rustJniLibsDir = layout.projectDirectory.dir("src/main/jniLibs")
val rustLibraries = rustAbis.map { rustJniLibsDir.file("$it/lib$game.so") }
val cargoBinDir = file("${System.getProperty("user.home")}/.cargo/bin")
val cargoExecutable = providers.gradleProperty("cargo")
    .orElse(cargoBinDir.resolve("cargo").absolutePath)
    .get()

android {
    namespace = "com.example.langameserver"
    compileSdk = 35

    defaultConfig {
        applicationId = gameConfig.applicationId
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        buildConfigField("String", "GAME_ID", "\"$game\"")
        buildConfigField("String", "RUST_LIBRARY", "\"$game\"")
        buildConfigField("int", "SERVER_PORT", gameConfig.port.toString())

        ndk {
            abiFilters += rustAbis
        }
    }

    buildFeatures {
        buildConfig = true
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDir(rustJniLibsDir)
        }
    }
}

kotlin {
    jvmToolchain(17)
}

val buildRustGame by tasks.registering(Exec::class) {
    group = "rust"
    description = "Builds the $game Rust websocket server as Android native libraries."
    workingDir = rustProjectDir.asFile
    val rustPath = listOf(cargoBinDir.absolutePath, System.getenv("PATH").orEmpty())
        .filter { it.isNotEmpty() }
        .joinToString(File.pathSeparator)
    environment("PATH", rustPath)
    commandLine(
        listOf(cargoExecutable, "ndk") +
            rustAbis.flatMap { listOf("-t", it) } +
            listOf(
                "--platform",
                "26",
                "-o",
                rustJniLibsDir.asFile.absolutePath,
                "build",
                "--release",
                "--features",
                "android-jni",
            ),
    )

    inputs.files(
        fileTree(rustProjectDir) {
            include("Cargo.toml")
            include("src/**/*.rs")
        },
        fileTree(rustCommonDir) {
            include("Cargo.toml")
            include("src/**/*.rs")
        },
        fileTree(rustShareTypesDir) {
            include("Cargo.toml")
            include("src/**/*.rs")
        },
    )
    outputs.files(rustLibraries)

    doFirst {
        rustJniLibsDir.asFile.deleteRecursively()
        rustJniLibsDir.asFile.mkdirs()
        val cargoNdkAvailable = runCatching {
            val cargoNdk = ProcessBuilder(cargoExecutable, "ndk", "--version")
                .redirectErrorStream(true)
                .apply { environment()["PATH"] = rustPath }
                .start()
            cargoNdk.waitFor() == 0
        }.getOrDefault(false)
        if (!cargoNdkAvailable) {
            throw GradleException(
                "cargo-ndk is required. Install it with `cargo install cargo-ndk`, " +
                    "or pass `-Pcargo=/path/to/cargo`.",
            )
        }
    }
}

if (!skipRustBuild) {
    tasks.named("preBuild") {
        dependsOn(buildRustGame)
    }
}
