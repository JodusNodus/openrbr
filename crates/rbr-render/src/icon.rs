//! ReRBR's icon on its windows, for the Linux and Windows taskbars (macOS shows the bundle's, `scripts/package.sh`),
//! and the app id the Linux desktop file matches.

/// The Wayland app id and X11 class of ReRBR's windows: the AppImage's `rerbr.desktop`, whose icon a Wayland
/// compositor shows.
pub const APP_ID: &str = "rerbr";

/// `scripts/packaging/icon-256.png` (8-bit RGBA) as a window icon.
pub fn window_icon() -> Option<winit::window::Icon> {
    let png = include_bytes!("../../../scripts/packaging/icon-256.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(png)).read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(info.buffer_size());
    winit::window::Icon::from_rgba(rgba, info.width, info.height).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_decodes() {
        assert!(super::window_icon().is_some());
    }
}
