//! How the viewer's window presents its frames (`viewer --present`, the launcher's "Frame presentation"; ours).

/// The presentation the window asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Present {
    /// The newest frame at each refresh, with no tearing and no wait for it: the frame rate is not tied to the
    /// display's (the default).
    #[default]
    Mailbox,
    /// Vsync: each frame waits for a refresh, so a frame just over one refresh waits two.
    Fifo,
    /// Shown as soon as drawn, uncapped; may tear.
    Immediate,
}

impl Present {
    pub const ALL: [Present; 3] = [Present::Mailbox, Present::Fifo, Present::Immediate];

    /// The name in the launcher's settings and `viewer --present`.
    pub fn name(self) -> &'static str {
        match self {
            Present::Mailbox => "mailbox",
            Present::Fifo => "fifo",
            Present::Immediate => "immediate",
        }
    }

    pub fn parse(text: &str) -> Option<Present> {
        Present::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(text.trim()))
    }

    fn mode(self) -> wgpu::PresentMode {
        match self {
            Present::Mailbox => wgpu::PresentMode::Mailbox,
            Present::Fifo => wgpu::PresentMode::Fifo,
            Present::Immediate => wgpu::PresentMode::Immediate,
        }
    }

    /// The surface's mode: this one if `supported`, else mailbox, else fifo (which every surface has).
    pub fn pick(self, supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
        [self.mode(), wgpu::PresentMode::Mailbox]
            .into_iter()
            .find(|m| supported.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo)
    }
}

/// The window's colour format among the surface's `formats`: 8-bit linear (not sRGB-encoding) when the surface has
/// one (the game's D3D9 output was not gamma-encoded, and egui blends in gamma space), else 10-bit, else the first
/// linear one, else the first. A Vulkan surface may also list formats the renderer cannot draw in (`Rgba8Snorm`,
/// `Rgba16Unorm` without its feature) or the extended-linear `Rgba16Float`, which would show the frame darker, and
/// they need not come after the 8-bit ones.
pub fn surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    use wgpu::TextureFormat::{Bgra8Unorm, Rgb10a2Unorm, Rgba8Unorm};
    [Bgra8Unorm, Rgba8Unorm, Rgb10a2Unorm]
        .into_iter()
        .find(|f| formats.contains(f))
        .or_else(|| formats.iter().copied().find(|f| !f.is_srgb()))
        .unwrap_or(formats[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};

    #[test]
    fn names_round_trip() {
        for present in Present::ALL {
            assert_eq!(Present::parse(present.name()), Some(present));
        }
        assert_eq!(Present::parse("vsync"), None);
    }

    #[test]
    fn unsupported_modes_fall_back_to_mailbox_then_fifo() {
        assert_eq!(Present::Immediate.pick(&[Fifo, Immediate, Mailbox]), Immediate);
        assert_eq!(Present::Immediate.pick(&[Fifo, Mailbox]), Mailbox);
        // Metal: no mailbox.
        assert_eq!(Present::Mailbox.pick(&[Fifo, Immediate]), Fifo);
        assert_eq!(Present::Fifo.pick(&[Fifo, Immediate, Mailbox]), Fifo);
        assert_eq!(Present::Mailbox.pick(&[]), Fifo);
    }

    #[test]
    fn surface_format_prefers_8_bit_linear() {
        use wgpu::TextureFormat::*;
        // Metal.
        assert_eq!(surface_format(&[Bgra8Unorm, Bgra8UnormSrgb, Rgba16Float, Rgb10a2Unorm]), Bgra8Unorm);
        // Vulkan surfaces listing an HDR or unusable format first.
        assert_eq!(surface_format(&[Rgba16Float, Bgra8UnormSrgb, Bgra8Unorm]), Bgra8Unorm);
        assert_eq!(surface_format(&[Bgra8UnormSrgb, Rgba8Snorm, Rgba8UnormSrgb, Rgba8Unorm]), Rgba8Unorm);
        assert_eq!(surface_format(&[Rgba16Unorm, Rgb10a2Unorm, Bgra8UnormSrgb]), Rgb10a2Unorm);
        assert_eq!(surface_format(&[Bgra8UnormSrgb]), Bgra8UnormSrgb);
    }
}
