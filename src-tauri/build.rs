fn main() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut attributes = tauri_build::Attributes::new();
    if msvc {
        // The application manifest (Common Controls 6) is embedded by the
        // linker into every target, so unit test binaries start on Windows
        // too; tauri-build would embed it into the application only.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // Linker warnings, such as a manifest that cannot be read, fail the build.
        println!("cargo:rustc-link-arg=/WX");
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
