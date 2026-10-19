//! On Windows, the icon embedded in `rerbr.exe` (scripts/packaging/icon.ico).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/packaging/icon.ico");
        println!("cargo::rerun-if-changed={icon}");
        winresource::WindowsResource::new().set_icon(icon).compile_for("rerbr").expect("rerbr.exe's icon resource");
    }
}
