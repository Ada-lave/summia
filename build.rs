use std::process::Command;

fn main() {
    // Only run on macOS
    if cfg!(target_os = "macos") {
        add_swift_runtime_paths();
    }
}

fn add_swift_runtime_paths() {
    // Try to find Swift runtime libraries using xcrun
    if let Ok(output) = Command::new("xcrun").args(&["--show-sdk-path"]).output() {
        if output.status.success() {
            let sdk_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

            // Add Swift library paths relative to SDK
            let swift_paths = vec![
                format!("{}/usr/lib/swift", sdk_path),
                "/usr/lib/swift".to_string(),
            ];

            for path in swift_paths {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path);
            }
        }
    }

    // Try to find Xcode toolchain Swift libraries
    if let Ok(output) = Command::new("xcrun").args(&["--find", "swift"]).output() {
        if output.status.success() {
            let swift_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

            // Extract toolchain path from swift binary path
            // Typical path: /Applications/Xcode.app/.../usr/bin/swift
            if let Some(toolchain_idx) = swift_path.find("Toolchains") {
                if let Some(usr_idx) = swift_path[toolchain_idx..].find("/usr/bin") {
                    let toolchain_lib = format!(
                        "{}/lib/swift/macosx",
                        &swift_path[..toolchain_idx + usr_idx + 4] // +4 for "/usr"
                    );
                    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", toolchain_lib);
                }
            }
        }
    }

    // Fallback: common Swift library locations
    let fallback_paths = vec![
        "/Library/Developer/CommandLineTools/usr/lib/swift/macosx",
        "/usr/local/lib/swift",
    ];

    for path in fallback_paths {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path);
    }

    // Add @executable_path relative path for bundled distributions
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../lib");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
}
