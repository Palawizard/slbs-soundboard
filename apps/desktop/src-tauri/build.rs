fn main() {
    let mut attributes = tauri_build::Attributes::new();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icons/icon.ico");

        let temporary_icon = std::env::temp_dir().join("slbs-soundboard-build-icon.ico");
        std::fs::copy("icons/icon.ico", &temporary_icon)
            .expect("failed to prepare the Windows application icon");

        let windows = tauri_build::WindowsAttributes::new().window_icon_path(temporary_icon);
        attributes = attributes.windows_attributes(windows);
    }

    tauri_build::try_build(attributes).expect("failed to prepare the Tauri application build");
}
