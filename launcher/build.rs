fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "ArtCraft Studio");
        res.set("FileDescription", "ArtCraft Studio Launcher");
        if let Err(e) = res.compile() {
            // Icon embedding is cosmetic; never fail the build because of it.
            println!("cargo:warning=could not embed Windows icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=../registry.json");
}
