fn main() {
    println!("cargo:rerun-if-changed=assets/itool.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/itool.ico")
            .set("ProductName", "ITool")
            .set("FileDescription", "ITool — iPhone original media exporter")
            .compile()
            .expect("Could not embed the Windows application icon");
    }
}
