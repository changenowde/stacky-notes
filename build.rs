// Bettet unter Windows das App-Icon in die .exe ein.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Stacky Notes");
        res.set("FileDescription", "Stacky Notes");
        res.compile().expect("Windows-Ressourcen konnten nicht eingebettet werden");
    }
}
