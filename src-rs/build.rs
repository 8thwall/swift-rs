#![allow(dead_code)]
use std::{env, fmt::Display, path::Path, path::PathBuf, process::Command};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftTarget {
    triple: String,
    unversioned_triple: String,
    module_triple: String,
    //pub swift_runtime_compatibility_version: String,
    #[serde(rename = "librariesRequireRPath")]
    libraries_require_rpath: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftPaths {
    runtime_library_paths: Vec<String>,
    runtime_library_import_paths: Vec<String>,
    runtime_resource_path: String,
}

#[derive(Deserialize)]
struct SwiftEnv {
    target: SwiftTarget,
    paths: SwiftPaths,
}

impl SwiftEnv {
    fn new(
        minimum_macos_version: &str,
        minimum_ios_version: Option<&str>,
        minimum_visionos_version: Option<&str>,
    ) -> Self {
        println!("cargo:warning=swift-rs: [build@new] minimum_macos_version: {minimum_macos_version:?}");
        println!("cargo:warning=swift-rs: [build@new] minimum_ios_version: {minimum_ios_version:?}");
        println!("cargo:warning=swift-rs: [build@new] minimum_visionos_version: {minimum_visionos_version:?}");

        let rust_target = RustTarget::from_env();
        let target = rust_target.swift_target_triple(
            minimum_macos_version,
            minimum_ios_version,
            minimum_visionos_version,
        );
        println!("cargo:warning=swift-rs: [build@new] target: {target:?}");

        let swift_target_info_str = Command::new("swift")
            .args(["-target", &target, "-print-target-info"])
            .output()
            .unwrap()
            .stdout;
        println!("cargo:warning=swift-rs: [build@new] swift_target_info_str: {}", String::from_utf8_lossy(&swift_target_info_str));

        serde_json::from_slice(&swift_target_info_str).unwrap()
    }
}

#[allow(clippy::upper_case_acronyms)]
enum RustTargetOS {
    MacOS,
    IOS,
    VisionOS,
}

impl RustTargetOS {
    fn from_env() -> Self {
        match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
            "macos" => RustTargetOS::MacOS,
            "ios" => RustTargetOS::IOS,
            "visionos" => RustTargetOS::VisionOS,
            _ => panic!("unexpected target operating system"),
        }
    }

    fn to_swift(&self) -> &'static str {
        match self {
            Self::MacOS => "macosx",
            Self::IOS => "ios",
            Self::VisionOS => "xros",
        }
    }
}

impl Display for RustTargetOS {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacOS => write!(f, "macos"),
            Self::IOS => write!(f, "ios"),
            Self::VisionOS => write!(f, "visionos"),
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
enum SwiftSDK {
    MacOS,
    IOS,
    IOSSimulator,
    VisionOS,
    VisionOSSimulator,
}

impl SwiftSDK {
    fn from_os(os: &RustTargetOS) -> Self {
        let target = env::var("TARGET").unwrap();
        let simulator = target.ends_with("ios-sim")
            || target.ends_with("visionos-sim")
            || (target.starts_with("x86_64") && target.ends_with("ios"));

        match os {
            RustTargetOS::MacOS => Self::MacOS,
            RustTargetOS::IOS if simulator => Self::IOSSimulator,
            RustTargetOS::IOS => Self::IOS,
            RustTargetOS::VisionOS if simulator => Self::VisionOSSimulator,
            RustTargetOS::VisionOS => Self::VisionOS,
        }
    }

    fn clang_lib_extension(&self) -> &'static str {
        match self {
            Self::MacOS => "osx",
            Self::IOS => "ios",
            Self::IOSSimulator => "iossim",
            Self::VisionOS => "xros",
            Self::VisionOSSimulator => "xrsimulator",
        }
    }
}

impl Display for SwiftSDK {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacOS => write!(f, "macosx"),
            Self::IOSSimulator => write!(f, "iphonesimulator"),
            Self::IOS => write!(f, "iphoneos"),
            Self::VisionOSSimulator => write!(f, "xrsimulator"),
            Self::VisionOS => write!(f, "xros"),
        }
    }
}

struct RustTarget {
    arch: String,
    os: RustTargetOS,
    sdk: SwiftSDK,
}

impl RustTarget {
    fn from_env() -> Self {
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        let os = RustTargetOS::from_env();
        let sdk = SwiftSDK::from_os(&os);

        Self { arch, os, sdk }
    }

    fn swift_target_triple(
        &self,
        minimum_macos_version: &str,
        minimum_ios_version: Option<&str>,
        minimum_visionos_version: Option<&str>,
    ) -> String {
        let unversioned = self.unversioned_swift_target_triple();
        format!(
            "{unversioned}{}{}",
            match &self.os {
                RustTargetOS::MacOS => minimum_macos_version,
                RustTargetOS::IOS => minimum_ios_version.unwrap(),
                RustTargetOS::VisionOS => minimum_visionos_version.unwrap(),
            },
            // simulator suffix
            matches!(
                self.sdk,
                SwiftSDK::IOSSimulator | SwiftSDK::VisionOSSimulator
            )
            .then(|| "-simulator".to_string())
            .unwrap_or_default()
        )
    }

    fn unversioned_swift_target_triple(&self) -> String {
        format!(
            "{}-apple-{}",
            match self.arch.as_str() {
                "aarch64" => "arm64",
                a => a,
            },
            self.os.to_swift(),
        )
    }
}

struct SwiftPackage {
    name: String,
    path: PathBuf,
}

/// Builder for linking the Swift runtime and custom packages.
#[cfg(feature = "build")]
pub struct SwiftLinker {
    packages: Vec<SwiftPackage>,
    macos_min_version: String,
    ios_min_version: Option<String>,
    visionos_min_version: Option<String>,
}

impl SwiftLinker {
    /// Creates a new [`SwiftLinker`] with a minimum macOS verison.
    ///
    /// Minimum macOS version must be at least 10.13.
    pub fn new(macos_min_version: &str) -> Self {
        Self {
            packages: vec![],
            macos_min_version: macos_min_version.to_string(),
            ios_min_version: None,
            visionos_min_version: None,
        }
    }

    /// Instructs the [`SwiftLinker`] to also compile for iOS
    /// using the specified minimum iOS version.
    ///
    /// Minimum iOS version must be at least 11.
    pub fn with_ios(mut self, min_version: &str) -> Self {
        self.ios_min_version = Some(min_version.to_string());
        self
    }

    /// Instructs the [`SwiftLinker`] to also compile for visionOS
    /// using the specified minimum visionOS version.
    ///
    /// Minimum visionOS version must be at least 11.
    pub fn with_visionos(mut self, min_version: &str) -> Self {
        self.visionos_min_version = Some(min_version.to_string());
        self
    }

    /// Adds a package to be linked against.
    /// `name` should match the `name` field in your `Package.swift`,
    /// and `path` should point to the root of your Swift package relative
    /// to your crate's root.
    pub fn with_package(mut self, name: &str, path: impl AsRef<Path>) -> Self {
        self.packages.extend([SwiftPackage {
            name: name.to_string(),
            path: path.as_ref().into(),
        }]);

        self
    }

    // Gets the sysroot from cc by looking for the -sysroot flag. Used b/c neither SDKROOT nor
    // SYSROOT environment variables are set when running using hermetic bazel.
    fn sysroot_from_cc(&self, sdk: &str) -> Option<String> {
        match sdk {
            "macosx" => {
                if let Ok(sdkroot) = std::env::var("OSX_SDK_ROOT") {
                    println!("cargo:warning=cc: [build@sysroot_from_cc]: OSX_SDK_ROOT: {sdkroot}");
                    return Some(sdkroot);
                }
            }
            "iphonesimulator" => {
                if let Ok(sdkroot) = std::env::var("IPHONE_SIMULATOR_SDK_ROOT") {
                    println!("cargo:warning=cc: [build@sysroot_from_cc]: IPHONE_SIMULATOR_SDK_ROOT: {sdkroot}");
                    return Some(sdkroot);
                }
            }
            "iphoneos" => {
                if let Ok(sdkroot) = std::env::var("IPHONE_SDK_ROOT") {
                    println!("cargo:warning=cc: [build@sysroot_from_cc]: IPHONE_SDK_ROOT: {sdkroot}");
                    return Some(sdkroot);
                }
            }
            _ => {}
        }
        // Print out all environment variables
        // for (key, value) in std::env::vars() {
        //     println!("cargo:warning=swift-rs: [build@sysroot_from_cc] {key}: {value:?}");
        // }

        // println!("cargo:warning=swift-rs: [build@sysroot_from_cc] OSX_SDK_ROOT: {}", std::env::var("OSX_SDK_ROOT").unwrap_or_default());
        // if let Some(sdkroot) = std::env::var("OSX_SDK_ROOT").ok() {
        //     println!("cargo:warning=cc: [build@sysroot_from_cc]: sdkroot: {sdkroot:?}");
        //     if !sdkroot.is_empty() {
        //         println!("cargo:warning=swift-rs: [build@sysroot_from_cc] SDKROOT is not empty");
        //         return Some(sdkroot);
        //     }
        // }

        let cc = std::env::var("CC").ok()?;
        // println!("cargo:warning=swift-rs: [build@sysroot_from_cc] cc: {cc:?}");
        let out = Command::new(cc)
            .args(["-v", "-E", "-"])
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        // println!("cargo:warning=swift-rs: [build@sysroot_from_cc] out: {out:?}");
        let s = std::str::from_utf8(&out.stderr).ok()?;
        // println!("cargo:warning=swift-rs: [build@sysroot_from_cc] s: {s:?}");
        let mut it = s.split_whitespace();
        while let Some(t) = it.next() {
            // println!("cargo:warning=swift-rs: [build@sysroot_from_cc] t: {t:?}");
            if t == "-isysroot" {
                if let Some(p) = it.next() {
                    return Some(p.to_string());
                }
            }
        }
        println!("cargo:warning=swift-rs: [build@sysroot_from_cc] None");
        None
    }

    /// Links the Swift runtime, then builds and links the provided packages.
    /// This does not (yet) automatically rebuild your Swift files when they are modified,
    /// you'll need to modify/save your `build.rs` file for that.
    pub fn link(self) {
        println!("cargo:warning=swift-rs: [build@link] ------------------------");

        let swift_env = SwiftEnv::new(
            &self.macos_min_version,
            self.ios_min_version.as_deref(),
            self.visionos_min_version.as_deref(),
        );

        #[allow(clippy::uninlined_format_args)]
        for path in swift_env.paths.runtime_library_paths {
            println!("cargo:warning=swift-rs: [build@link] runtime_library_path: {path:?}");
            println!("cargo:rustc-link-search=native={path}");
        }
        // println!("cargo:rustc-link-search=native=/Users/paris/Downloads/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/iphoneos");

        let debug = env::var("DEBUG").unwrap() == "true";
        println!("cargo:warning=swift-rs: [build@link] debug: {debug:?}");
        let configuration = if debug { "debug" } else { "release" };
        println!("cargo:warning=swift-rs: [build@link] configuration: {configuration:?}");
        let rust_target = RustTarget::from_env();

        link_clang_rt(&rust_target);

        for package in &self.packages {
            let package_path =
                Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join(&package.path);
            println!("cargo:warning=swift-rs: [build@link] package_path: {package_path:?}");
            let out_path = Path::new(&env::var("OUT_DIR").unwrap())
                .join("swift-rs")
                .join(&package.name);
            println!("cargo:warning=swift-rs: [build@link] out_path: {out_path:?}");

            // NOTE(paris): Fetch the SDK directly from cc because when running using hermetic
            // bazel, we do not have xcrun available in the xcode toolchain (it's an OSX host
            // library). So running it will use non-hermetic xcode toolchain and fail on CI machines
            // where it is not installed (and give incorrect paths even if it is installed).
            println!("cargo:warning=swift-rs: [build@link] rust_target.sdk: {}", rust_target.sdk.to_string());
            let sdk_path_from_cc = self.sysroot_from_cc(&rust_target.sdk.to_string());
            println!("cargo:warning=swift-rs: [build@link] sdk_path_from_cc: {sdk_path_from_cc:?}");
            let sdk_path = if let Some(path) = sdk_path_from_cc {
                println!("cargo:warning=swift-rs: [build@link] path: {path:?}");
                path
            } else {
                println!("cargo:warning=swift-rs: [build@link] xcrun");
                let sdk_path_output = Command::new("xcrun")
                    .args(["--sdk", &rust_target.sdk.to_string(), "--show-sdk-path"])
                    .output()
                    .unwrap();
                println!("cargo:warning=swift-rs: [build@link] sdk_path_output: {sdk_path_output:?}");

                if !sdk_path_output.status.success() {
                    panic!(
                        "Failed to get SDK path with `xcrun --sdk {} --show-sdk-path`",
                        rust_target.sdk
                    );
                }

                String::from_utf8(sdk_path_output.stdout)
                    .unwrap()
                    .trim()
                    .to_string()
            };
            println!("cargo:warning=swift-rs: [build@link] sdk_path: {sdk_path:?}");

            let mut command = Command::new("swift");
            println!("cargo:warning=swift-rs: [build@link] command: {command:?}");
            command.current_dir(&package.path);
            println!("cargo:warning=swift-rs: [build@link] package.path: {}", package.path.display());

            let arch = match std::env::consts::ARCH {
                "aarch64" => "arm64",
                arch => arch,
            };
            println!("cargo:warning=swift-rs: [build@link] arch: {arch:?}");

            let swift_target_triple = rust_target.swift_target_triple(
                &self.macos_min_version,
                self.ios_min_version.as_deref(),
                self.visionos_min_version.as_deref(),
            );
            println!("cargo:warning=swift-rs: [build@link] swift_target_triple: {swift_target_triple:?}");

            command
                // Build the package (duh)
                .arg("build")
                // NOTE(paris): Disable the sandbox because we build inside the Bazel sandbox, which
                // has its own sandbox. Without this we fail with:
                // - `sandbox-exec: sandbox_apply: Operation not permitted`
                .arg("--disable-sandbox")
                // SDK path for regular compilation (idk)
                .args(["--sdk", sdk_path.trim()])
                // Release/Debug configuration
                .args(["-c", configuration])
                .args(["--arch", arch])
                // Where the artifacts will be generated to
                .args(["--build-path", &out_path.display().to_string()])
                // Override SDK path for each swiftc instance.
                // Necessary for iOS compilation.
                .args(["-Xswiftc", "-sdk"])
                .args(["-Xswiftc", sdk_path.trim()])
                // Override target triple for each swiftc instance.
                // Necessary for iOS compilation.
                .args(["-Xswiftc", "-target"])
                .args(["-Xswiftc", &swift_target_triple])
                .args(["-Xcc", &format!("--target={swift_target_triple}")])
                .args(["-Xcxx", &format!("--target={swift_target_triple}")]);

            println!("cargo:warning=swift-rs: [build@link] command.status(): {}", command.status().unwrap());

            if !command.status().unwrap().success() {
                println!("cargo:warning=swift-rs: [build@link] Failed to compile swift package {}", package.name);
                panic!("Failed to compile swift package {}", package.name);
            }

            let search_path = out_path
                // swift build uses this output folder no matter what is the target
                .join(format!("{}-apple-macosx", arch))
                .join(configuration);
            println!("cargo:warning=swift-rs: [build@link] package_path: {}", package_path.display());
            println!("cargo:warning=swift-rs: [build@link] search_path: {}", search_path.display());
            println!("cargo:warning=swift-rs: [build@link] package.name: {}", package.name);

            println!("cargo:rerun-if-changed={}", package_path.display());
            println!("cargo:rustc-link-search=native={}", search_path.display());
            println!("cargo:rustc-link-lib=static={}", package.name);

            println!("cargo:warning=swift-rs: [build@link] DONE! ------------------------");
        }
    }
}

fn link_clang_rt(rust_target: &RustTarget) {
    println!(
        "cargo:rustc-link-lib=clang_rt.{}",
        rust_target.sdk.clang_lib_extension()
    );
    println!("cargo:rustc-link-search={}", clang_link_search_path());
}

fn clang_link_search_path() -> String {
    let output = std::process::Command::new(
        std::env::var("SWIFT_RS_CLANG").unwrap_or_else(|_| "/usr/bin/clang".to_string()),
    )
    .arg("--print-search-dirs")
    .output()
    .unwrap();
    println!("cargo:warning=swift-rs: [build@link_clang_rt] output: {output:?}");
    if !output.status.success() {
        panic!("Can't get search paths from clang");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1).unwrap();
            return format!("{}/lib/darwin", path);
        }
    }
    panic!("clang is missing search paths");
}
