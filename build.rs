fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    std::env::set_current_dir(&manifest_dir).expect("failed to set current dir");
    println!("cargo:rerun-if-changed=assets\\icon.ico");
    println!("cargo:rerun-if-changed=src\\app.manifest");
    let mut res = winres::WindowsResource::new();
    res.set_icon(&format!(r#"{manifest_dir}\assets\icon.ico"#));
    res.set_manifest_file("src\\app.manifest");
    res.set_version_info(winres::VersionInfo::FILEVERSION, 0x0001_0000_0000_0000);
    res.compile().expect("failed to compile Windows resources");
}
