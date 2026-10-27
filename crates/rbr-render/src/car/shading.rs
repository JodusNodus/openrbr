//! A face's look: its shader class's lighting constants, dirt and scrape textures, render state, pass and draw list.

use crate::stage::{Address, DrawState, Shade, TextureRef};
use rbr_formats::ini::Section;

/// The lighting constants of `class` in the car's clear-weather shader settings (the overcast file is not used), or
/// `None` (unlit) when the class has no section.
pub(super) fn shade_for(settings: &[Section], class: &str) -> Option<Shade> {
    let section = settings.iter().find(|s| s.name == class)?;
    let number = |key: &str| section.get(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    Some(Shade {
        ka: number("Ka"),
        kd: number("Kd"),
        ks: number("CookTorranceKs"),
        m: number("CookTorranceM"),
        fresnel_min: number("FresnelMin"),
        fresnel_max: number("FresnelMax"),
        kf: number("Kf"),
        glass: class == "CarGlass",
        tint: class == "CarIntGlass",
        glow: false,
        crack: false,
        dirt: None,
        grime: 0,
        spatter: false,
    })
}

/// The friendly name of the dirt mask a car class blends in by the dirt level (`Shade::dirt`).
pub(super) fn dirt_texture(shader: &str) -> Option<&'static str> {
    match shader {
        "CarPaint" | "CarTexture" => Some("Dirt"),
        "CarTire" => Some("TireDirt"),
        _ => None,
    }
}

/// The friendly name of the scrape texture a car class blends in by the vertex's scrape amount: `Scrape` for
/// `CarPaint` (stage 3) and `InternalScrape` for `CarPaintInt` (stage 2). The other classes bind `Scrape` and do not
/// sample it (`notes/formats.md` "Shaders and vertex layouts").
pub(super) fn scrape_texture(shader: &str) -> Option<&'static str> {
    match shader {
        "CarPaint" => Some("Scrape"),
        "CarPaintInt" => Some("InternalScrape"),
        _ => None,
    }
}

/// The render state of a car face: lit by `shade`, depth tested, and the state `pass` gives. A `scrape` texture is
/// the second texture, blended in by its alpha and the vertex's scrape amount (`CarPaint1.pso`: `lrp` by
/// `sat(v0.w * t3.a)` towards `t3 * v0`).
pub(super) fn draw_state(
    texture: Option<TextureRef>,
    scrape: Option<TextureRef>,
    pass: Pass,
    shade: Option<Shade>,
) -> DrawState {
    let blend = pass != Pass::Opaque;
    DrawState {
        textures: [texture, scrape],
        shadow: None,
        address: [if pass == Pass::Crack { Address::Clamp } else { Address::Wrap }; 2],
        second_texture: scrape.is_some(),
        alpha_ref: (blend && pass != Pass::Crack).then_some(0),
        // `CShaderCarBase::Load` sets the diffuse sampler's bias for every class with a diffuse bitmap.
        mip_bias: texture.is_some(),
        cull_back: !matches!(pass, Pass::Plastic | Pass::InteriorGlass),
        blend,
        additive: false,
        depth_test: true,
        depth_write: matches!(pass, Pass::Opaque | Pass::Plastic),
        shade,
        specular: None,
    }
}

/// How a face is drawn. Every car class's `Load` runs `CShaderCarBase::Load` (0x1007b3e0), whose
/// `SShaderMaterial::SetupRenderState` (0x1003f4b0) turns on, for a material with an opacity map (or a transparency
/// above 0, which no retail car has), alpha blending (`SRCALPHA`, `INVSRCALPHA`), the alpha test `> 0` and no z
/// write; the render state's default culls back faces (`D3DCULL_CCW`, 0x10091b50). `CarPlastic`'s `Load` (0x1007dab0)
/// then turns z write on and culling off when it is transparent, and `CarIntGlass`'s turns culling off. The class
/// does not matter otherwise: the capture's `CarTexture` draw of `internal transparent::tire` (an opacity material, in
/// the opaque list) is blended with test `> 0` and no z write, as its `CarGlass` draws are (`notes/formats.md` "Shaders and vertex layouts").
/// The pixel shaders' alpha is the diffuse texture's (`CarTextureNoDirt.pso`: `mov r0.w, t0.w`); `CarGlass` adds a
/// Fresnel term, which is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Opaque,
    /// Blended, alpha test `> 0`, no z write, back faces culled.
    Blended,
    /// `CarPlastic` with an opacity map: blended, alpha test `> 0`, z write, no culling.
    Plastic,
    /// `CarIntGlass`: blended, alpha test `> 0`, no z write, no culling.
    InteriorGlass,
    /// A window's crack overlay (`m_damageRes`, `InitDamage`): blended, no alpha test, back faces culled, clamped uvs.
    /// The game writes z (with `LESSEQUAL`); the overlay does not, so the second half's overlay on the same pane is
    /// not hidden by the first under the renderer's `Less`.
    Crack,
}

/// The pass of a face with shader class `shader`, whose material has an opacity map when `opacity`.
pub(super) fn pass_for(shader: &str, opacity: bool) -> Pass {
    match (shader, opacity) {
        ("CarIntGlass", _) => Pass::InteriorGlass,
        ("CarPlastic", true) => Pass::Plastic,
        (_, true) => Pass::Blended,
        (_, false) => Pass::Opaque,
    }
}

/// The car list a face's shader is drawn from (`FUN_1001e430`, rbr-recomp `notes/formats/sgc.md`, section 9). The world render
/// (0x1006ada0) draws the wheel list (`CarRim`, `CarTire`) and then `CHH_Car::Render`, which draws the opaque list
/// and then the transparent one. A shader goes to the transparent list when its class's `IsTransparent` is true:
/// `CarIntGlass` always, `CarGlass`, `CarPlastic`, `CarRim` and `CarLight` with an opacity map; the other classes
/// return false, so a blended `CarTextureNoDirt` face (the `pc_e_window_*` panes) is drawn in the opaque list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum List {
    Wheels,
    Opaque,
    Transparent,
}

pub(super) fn list_for(shader: &str, opacity: bool) -> List {
    match shader {
        "CarRim" | "CarTire" => List::Wheels,
        "CarIntGlass" => List::Transparent,
        "CarGlass" | "CarPlastic" | "CarLight" if opacity => List::Transparent,
        _ => List::Opaque,
    }
}

/// When a face is drawn: its list, then its shader's rank in the order the loader created the shaders. Within a
/// shader the parts keep the model's order.
pub(super) type Order = (List, usize);

/// The diffuse and scrape textures, the pass and the draw order of one face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FaceStyle {
    pub(super) texture: Option<TextureRef>,
    pub(super) scrape: Option<TextureRef>,
    pub(super) shade: Option<Shade>,
    pub(super) pass: Pass,
    pub(super) order: Order,
}
