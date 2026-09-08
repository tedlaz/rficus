fn main() {
    // The exe's own icon, shown by Explorer and the taskbar. The window icon is
    // set separately at runtime (see ui.rs); this is the file's icon resource.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=ficus.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("ficus.ico");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not stop the build; the app just
            // keeps the default icon.
            println!("cargo:warning=could not embed ficus.ico: {e}");
        }
    }
}
