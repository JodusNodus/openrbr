//! On Windows, the icon embedded in `rerbr.exe` (scripts/packaging/icon.ico, from icon.py).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        let icon = "../../scripts/packaging/icon.ico";
        println!("cargo::rerun-if-changed={icon}");
        winresource::WindowsResource::new().set_icon(icon).compile().expect("rerbr.exe's icon resource");
    }
}
