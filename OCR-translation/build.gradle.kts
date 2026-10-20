plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Keep the original Kotlin package (dev.davidv.translator) untouched:
// the R class is generated under this namespace, so every bare `R`
// reference inside the pasted sources keeps resolving with zero renames.
android {
    namespace = "dev.davidv.translator"
    compileSdk {
        version = release(36)
    }
    ndkVersion = "27.0.12077973"

    sourceSets {
        getByName("main") {
            aidl.srcDir("src/main/aidl")
            java.srcDir(layout.buildDirectory.dir("generated/source/uniffi/kotlin"))
        }
    }

    defaultConfig {
        minSdk = 29

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    kotlinOptions {
        jvmTarget = "11"
    }
    buildFeatures {
        aidl = true
        compose = true
        buildConfig = true
    }

    lint {
        disable += "MissingTranslation"
    }
}

val bindingsRootDir = file("src/main/bindings")
val bindingsBindgenRootDir = file("bindings-bindgen")
val jniLibsRootDir = file("src/main/jniLibs")
val llvmDllDir =
    File(
        System.getenv("LOCALAPPDATA")
            ?: (System.getenv("USERPROFILE") + "/AppData/Local"),
        "llvm-dll",
    ).absolutePath
val ndkDir =
    System.getenv("ANDROID_NDK_ROOT")
        ?: System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("ANDROID_HOME")?.let { "$it/ndk/27.0.12077973" }
        ?: System.getenv("ANDROID_SDK_ROOT")?.let { "$it/ndk/27.0.12077973" }
        ?: (try { android.sdkDirectory.resolve("ndk/27.0.12077973").absolutePath } catch (_: Exception) { null })
        ?: throw GradleException("NDK 27.0.12077973 not found: set ANDROID_HOME or install it via sdkmanager")
val sdkCmakeBinDir =
    listOfNotNull(
        System.getenv("ANDROID_HOME"),
        System.getenv("ANDROID_SDK_ROOT"),
        File(ndkDir).parentFile?.parentFile?.absolutePath,
    ).asSequence()
        .map { File(File(it, "cmake"), "").listFiles()?.maxByOrNull { f -> f.name } }
        .filterNotNull()
        .map { File(it, "bin").absolutePath }
        .firstOrNull { File(it).exists() }
val bindingsAndroidApi = 29

fun jniLibAbiDir(abi: String) = File(jniLibsRootDir, abi)

// Short, space-free cargo home + target dir: absolute host paths get baked
// into ninja object dirs (260-char Windows limit), so both must stay short.
val shortCargoHome = "C:/cgo"
val shortCargoTargetDir = "C:/ct"

val ndkLlvmPrebuiltFw = "$ndkDir/toolchains/llvm/prebuilt/windows-x86_64".replace('\\', '/')
val ndkClangIncludeFw =
    fileTree("$ndkDir/toolchains/llvm/prebuilt/windows-x86_64") {
        include("lib/clang/*/include/stddef.h")
    }.files.firstOrNull()?.parent?.replace('\\', '/')
        ?: error("clang include dir not found under $ndkDir")
// Read by espeak-rs-sys' build script (BINDGEN_EXTRA_CLANG_ARGS_<target>);
// plain CFLAGS must NOT be set: they leak into cmake configure checks.
val bindgenExtraArgs =
    listOf(
        "--target=aarch64-linux-android29",
        "--sysroot=$ndkLlvmPrebuiltFw/sysroot",
        "-isystem$ndkClangIncludeFw",
        "-isystem${ndkDir.replace('\\', '/')}/sources/cxx-stl/llvm-libc++/include",
    ).joinToString(" ")
val msysBinDir = "C:/msys64/usr/bin"

// NDK clang version differs per NDK release: discover the runtime-builtins
// archive instead of hardcoding the clang version directory.
fun clangRtLib(abi: String): String {
    val rtArch =
        when (abi) {
            "arm64-v8a" -> "aarch64-android"
            "armeabi-v7a" -> "arm-android"
            "x86" -> "i686-android"
            "x86_64" -> "x86_64-android"
            else -> error("Unknown abi $abi")
        }
    val hostOs =
        when {
            org.gradle.internal.os.OperatingSystem.current().isWindows -> "windows-x86_64"
            org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "darwin-x86_64"
            else -> "linux-x86_64"
        }
    val hits =
        fileTree("$ndkDir/toolchains/llvm/prebuilt/$hostOs") {
            include("lib/clang/*/lib/$hostOs/libclang_rt.builtins-$rtArch.a")
        }.files +
            fileTree("$ndkDir/toolchains/llvm/prebuilt/$hostOs") {
                include("lib/clang/*/lib/linux/libclang_rt.builtins-$rtArch.a")
            }.files
    return (hits.firstOrNull()?.absolutePath
        ?: error("libclang_rt for $abi not found under $ndkDir"))
}

fun cmakePathRemapFlags(): String =
    listOf(
        "-ffile-prefix-map=${rootProject.projectDir.absolutePath}=.",
        "-fdebug-prefix-map=${rootProject.projectDir.absolutePath}=.",
    ).joinToString(" ")

fun cargoEncodedRustflags(abi: String? = null): String {
    val base =
        listOf(
            "--remap-path-prefix=${rootProject.projectDir.absolutePath}=.",
        )
    if (abi == null) return base.joinToString("\u001F")
    return (
        base +
            listOf(
                "-C",
                "link-arg=-Wl,--threads=1",
                "-C",
                "link-arg=-Wl,--sort-section=name",
                "-C",
                "link-arg=-Wl,--sort-common",
                "-C",
                "link-arg=-Wl,-z,max-page-size=16384",
                "-C",
                "link-arg=${clangRtLib(abi)}",
            )
        ).joinToString("\u001F")
}

val abiToTaskSuffix =
    mapOf(
        "arm64-v8a" to "Aarch64",
        "armeabi-v7a" to "ArmeabiV7a",
        "x86_64" to "X86_64",
        "x86" to "X86",
    )

val abiToCargoTarget =
    mapOf(
        "arm64-v8a" to "aarch64-linux-android",
        "armeabi-v7a" to "armv7-linux-androideabi",
        "x86_64" to "x86_64-linux-android",
        "x86" to "i686-linux-android",
    )

val cargoProfileArgs = listOf("--release")

val abiToBindingsTask =
    abiToCargoTarget.mapValues { (abi, cargoTarget) ->
        val taskSuffix = abiToTaskSuffix.getValue(abi)
        tasks.register("buildBindings$taskSuffix") {
            group = "build"
            description = "Build Rust bindings library for $abi"
            inputs.file(bindingsRootDir.resolve("Cargo.toml"))
            inputs.file(bindingsRootDir.resolve("Cargo.lock"))
            inputs.file(bindingsRootDir.resolve(".cargo/config.toml"))
            inputs.dir(bindingsRootDir.resolve("src"))
            inputs.property("cargoTarget", cargoTarget)
            inputs.property("androidApi", bindingsAndroidApi)
            inputs.property("androidNdkRoot", ndkDir)
            inputs.property("cargoEncodedRustflags", cargoEncodedRustflags(abi))
            inputs.property("cargoProfile", "release")
            outputs.file(File(jniLibAbiDir(abi), "libbindings.so"))
            outputs.file(File(jniLibAbiDir(abi), "libc++_shared.so"))

            doLast {
                exec {
                    workingDir = bindingsRootDir
                    environment("ANDROID_NDK_ROOT", ndkDir)
                    environment("ANDROID_NDK_HOME", ndkDir)
                    environment("LIBCLANG_PATH", llvmDllDir)
                    environment("CARGO_HOME", shortCargoHome)
                    environment("CARGO_TARGET_DIR", shortCargoTargetDir)
                    environment("BINDGEN_EXTRA_CLANG_ARGS_aarch64-linux-android", bindgenExtraArgs)
                    environment("BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android", bindgenExtraArgs)
                    val extraPath =
                        listOfNotNull(
                            sdkCmakeBinDir,
                            msysBinDir.takeIf { File(it).exists() },
                        ).joinToString(File.pathSeparator)
                    if (extraPath.isNotEmpty()) {
                        environment(
                            "PATH",
                            extraPath + File.pathSeparator + (System.getenv("PATH") ?: ""),
                        )
                    }
                    environment("CARGO_ENCODED_RUSTFLAGS", cargoEncodedRustflags(abi))
                    environment(
                        "CMAKE_TOOLCHAIN_FILE",
                        File(ndkDir, "build/cmake/android.toolchain.cmake").absolutePath,
                    )
                    environment("ANDROID_ABI", abi)
                    environment("ANDROID_PLATFORM", "android-$bindingsAndroidApi")
                    environment("CMAKE_GENERATOR", "Ninja")
                    commandLine(
                        "cargo",
                        "ndk",
                        "build",
                        "--lib",
                        "--target",
                        cargoTarget,
                        *cargoProfileArgs.toTypedArray(),
                        "--platform",
                        bindingsAndroidApi.toString(),
                        "--link-libcxx-shared",
                        "--output-dir",
                        "../jniLibs",
                    )
                }
            }
        }
    }

val targetAbi = project.findProperty("targetAbi")?.toString()
val selectedAbis =
    if (targetAbi != null) {
        listOf(targetAbi)
    } else {
        listOf("arm64-v8a")
    }
val bindingsTasks = selectedAbis.mapNotNull { abiToBindingsTask[it] }

val bindgenHostBinary =
    File(
        bindingsBindgenRootDir,
        if (org.gradle.internal.os.OperatingSystem.current().isWindows) {
            "target/release/uniffi-bindgen.exe"
        } else {
            "target/release/uniffi-bindgen"
        },
    )

val buildUniffiBindgen =
    tasks.register("buildUniffiBindgen", Exec::class) {
        group = "build"
        description = "Build the host uniffi-bindgen binary"
        workingDir = bindingsBindgenRootDir
        environment("CARGO_HOME", shortCargoHome)
        inputs.file(bindingsBindgenRootDir.resolve("Cargo.toml"))
        inputs.dir(bindingsBindgenRootDir.resolve("src"))
        outputs.file(bindgenHostBinary)
        commandLine("cargo", "build", "--release")
    }

val generatedBindingsDir = layout.buildDirectory.dir("generated/source/uniffi/kotlin")

val bindgenSourceAbi = selectedAbis.first()
val bindgenSourceLib = File(jniLibAbiDir(bindgenSourceAbi), "libbindings.so")

val generateUniffiBindings =
    tasks.register("generateUniffiBindings", Exec::class) {
        group = "build"
        description = "Generate Kotlin bindings from the compiled libbindings.so"
        dependsOn(buildUniffiBindgen)
        dependsOn(abiToBindingsTask.getValue(bindgenSourceAbi))
        workingDir = bindingsRootDir
        inputs.file(bindgenHostBinary)
        inputs.file(bindgenSourceLib)
        outputs.dir(generatedBindingsDir)
        doFirst {
            val outDir = generatedBindingsDir.get().asFile
            outDir.deleteRecursively()
            outDir.mkdirs()
        }
        commandLine(
            bindgenHostBinary.absolutePath,
            "generate",
            bindgenSourceLib.absolutePath,
            "--language",
            "kotlin",
            "--out-dir",
            generatedBindingsDir.get().asFile.absolutePath,
            "--no-format",
            "--metadata-no-deps",
        )
    }

tasks.named("preBuild") {
    dependsOn(bindingsTasks)
    dependsOn(generateUniffiBindings)
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.appcompat)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation("androidx.compose.material:material-icons-extended")
    implementation(libs.material)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.exifinterface)
    // Test-only: unused in main sources; androidx.test.uiautomator pulls junit
    // onto any runtime classpath it is declared with, leaking it into the APK.
    androidTestImplementation(libs.androidx.uiautomator)
    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.coroutines.core)
    implementation(libs.androidx.datastore.preferences)
    implementation(libs.androidx.graphics.path)
    implementation(libs.kotlinx.serialization.json)
    implementation("com.github.yalantis:ucrop:2.2.11")
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation(libs.androidx.camera.core)
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
}
