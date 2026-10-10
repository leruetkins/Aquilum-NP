use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winres::WindowsResource::new();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let icon_path = Path::new(&manifest_dir).join("assets").join("iconi.ico");
    if icon_path.exists() {
        res.set_icon(icon_path.to_str().unwrap());
    } else {
        println!("cargo:warning=assets/iconi.ico not found");
    }

    if let Err(e) = res.compile() {
        println!("cargo:warning=winres compile failed: {e}");
    }

    // Резерв стека для GPUI на Windows (дефолтного 1 МБ может не хватать при глубокой вложенности)
    println!("cargo:rustc-link-arg=/STACK:33554432");
}
