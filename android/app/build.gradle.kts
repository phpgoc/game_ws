import org.gradle.api.GradleException
import org.gradle.api.tasks.Exec

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val rustAbis = providers.gradleProperty("rustAbis")
    .orNull
    ?.split(",")
    ?.map { it.trim() }
    ?.filter { it.isNotEmpty() }
    ?: listOf("arm64-v8a", "x86_64")
val rustProjectDir = layout.projectDirectory.dir("../../rust/landlord")
val rustCommonDir = layout.projectDirectory.dir("../../rust/common")
val rustShareTypesDir = layout.projectDirectory.dir("../../share_type_public")
val rustJniLibsDir = layout.projectDirectory.dir("src/main/jniLibs")
val rustLibraries = rustAbis.map { rustJniLibsDir.file("$it/liblandlord.so") }
val cargoBinDir = file("${System.getProperty("user.home")}/.cargo/bin")
val cargoExecutable = providers.gradleProperty("cargo")
    .orElse(cargoBinDir.resolve("cargo").absolutePath)
    .get()

android {
    namespace = "com.example.landlordserver"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.example.landlordserver"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += rustAbis
        }
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

val buildRustLandlord by tasks.registering(Exec::class) {
    group = "rust"
    description = "Builds the Rust landlord websocket server as an Android native library."
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
        val cargoNdkAvailable = runCatching {
            val cargoNdk = ProcessBuilder(cargoExecutable, "ndk", "--version")
                .redirectErrorStream(true)
                .apply { environment()["PATH"] = rustPath }
                .start()
            cargoNdk.waitFor() == 0
        }.getOrDefault(false)
        if (!cargoNdkAvailable) {
            throw GradleException(
                "cargo-ndk is required to build liblandlord.so. " +
                    "Expected cargo at `$cargoExecutable` and cargo-ndk in `$cargoBinDir`. " +
                    "Install it with `cargo install cargo-ndk`, or pass `-Pcargo=/path/to/cargo`. Then run " +
                    "`rustup target add aarch64-linux-android x86_64-linux-android`.",
            )
        }
    }
}

tasks.named("preBuild") {
    dependsOn(buildRustLandlord)
}
