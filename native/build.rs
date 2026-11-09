fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon_with_id("../assets/icon.ico", "101");
        resource.set("ProductName", "RustifiedYoutube");
        resource.set("FileDescription", "RustifiedYoutube");
        resource.compile().expect("compile Windows resources");
    }
}
