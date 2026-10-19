//! Part and material data of a car, from `<car>.ini` and `<car>_shaders.ini`.
//!
//! A part (`[e_body]`, `[e_wheel_tarmac_lf]`, ...) names one shader class and one material per face,
//! as `FaceBlock<N>_Shader` and `FaceBlock<N>_Material`, where `N` is the face's index in the
//! model's mesh (rbr-recomp `notes/formats/sgc.md`, section 8). `Switch = true` marks parts that the car shows
//! only in some states (lights, brake discs). A material is drawn with its diffuse bitmap, found
//! through the `<material>_diffuse_bitmap` section and the `TextureSet0` texture set
//! (`notes/formats.md` "Cars", `notes/formats.md` "Shaders and vertex layouts").

use crate::ini::Section;

/// One face's shader class and material, as the part section names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceBlock {
    /// Shader class, for example `CarPaint` or `CarTire`.
    pub shader: String,
    /// Material name, for example `external::carpaint`. Equal to the face's name in the model.
    pub material: String,
}

/// The section named `name`, ignoring ASCII case. The game matches section names this way.
pub fn section<'a>(sections: &'a [Section], name: &str) -> Option<&'a Section> {
    sections.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

/// The face blocks of a part, in face order: `FaceBlock0`, `FaceBlock1`, ... up to the first index
/// with no shader or material key.
pub fn face_blocks(part: &Section) -> Vec<FaceBlock> {
    (0..)
        .map_while(|n| {
            Some(FaceBlock {
                shader: part.get(&format!("FaceBlock{n}_Shader"))?.to_owned(),
                material: part.get(&format!("FaceBlock{n}_Material"))?.to_owned(),
            })
        })
        .collect()
}

/// Whether the part is shown only in some states (`Switch = true`: lights, brake discs, and so on).
pub fn is_switched(part: &Section) -> bool {
    part.get("Switch").is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

/// The DOS path (`cars\Xsara\textures\xsara_external.dds`) of a material's diffuse texture, or `None`
/// when the material has no bitmap. `shaders` is the sections of `<car>_shaders.ini`. As the game's car shaders
/// (`CShaderCarBase::Load`, HH 0x1007b3e0): the texture is the `[<material>_diffuse_bitmap]` section's `Filename`
/// whatever the material's own section says, or whether it has one (RSF's GR Yaris Rally2 has only the bitmap
/// sections for `internal::hud`).
pub fn diffuse_path<'a>(shaders: &'a [Section], material: &str) -> Option<&'a str> {
    let file = section(shaders, &format!("{material}_diffuse_bitmap"))?.get("Filename").filter(|f| !f.is_empty())?;
    texture_set(shaders).find_map(|(name, path)| name.eq_ignore_ascii_case(file).then_some(path))
}

/// The `TextureSet0` texture set's entries (file name, DOS path). Ours: the entries of every section with the set's
/// name, as the Fiesta Rally3 (RSF) has an empty `[TSDefault]` header before its own.
fn texture_set(shaders: &[Section]) -> impl Iterator<Item = (&str, &str)> {
    let name = section(shaders, "TextureSets").and_then(|s| s.get("TextureSet0"));
    shaders
        .iter()
        .filter(move |s| name.is_some_and(|n| s.name.eq_ignore_ascii_case(n)))
        .flat_map(|s| s.entries.iter().map(|(file, path)| (file.as_str(), path.as_str())))
}

/// The DOS path of the texture whose `FriendlyName` is `name` (`Scrape`, `InternalScrape`, `Dirt`, ...), which the
/// car shader classes bind by that name (`GetFriendlyTexture`, `notes/formats.md` "Shaders and vertex layouts"): the file's own section
/// carries the name, and the `TextureSet0` texture set its path.
pub fn friendly_path<'a>(shaders: &'a [Section], name: &str) -> Option<&'a str> {
    texture_set(shaders)
        .find(|(file, _)| section(shaders, file).and_then(|s| s.get("FriendlyName")) == Some(name))
        .map(|(_, path)| path)
}

/// Whether a material has an opacity channel (`OpacityChannel = true`). The glass classes are alpha
/// blended only when it does (`CarGlass` in the 1.02 shader, `notes/formats.md` "Shaders and vertex layouts").
pub fn opacity_channel(shaders: &[Section], material: &str) -> bool {
    section(shaders, material).and_then(|s| s.get("OpacityChannel")).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini;

    const PART: &str = "[e_body]\r\n\
        FaceBlock0_Material = external::carpaint\r\n\
        FaceBlock0_Shader = CarPaint\r\n\
        FaceBlock1_Material = external::tire\r\n\
        FaceBlock1_Shader = CarTexture\r\n\
        Predicate = NotUsed\r\n\
        Switch = false\r\n\
        [e_light_lf]\r\n\
        FaceBlock0_Material = external dynamic::tire\r\n\
        FaceBlock0_Shader = CarLight\r\n\
        Switch = TRUE\r\n";

    const SHADERS: &str = "[external::carpaint]\r\n\
        DiffuseChannel = true\r\n\
        [external::carpaint_diffuse_bitmap]\r\n\
        Filename = xsara_external.dds\r\n\
        [external::tire]\r\n\
        DiffuseChannel = false\r\n\
        [external transparent::carpaint]\r\n\
        OpacityChannel = true\r\n\
        [external::tire_diffuse_bitmap]\r\n\
        Filename = xsara_external.dds\r\n\
        [TextureSets]\r\n\
        TextureSet0 = TSDefault\r\n\
        [TSDefault]\r\n\
        xsara_external.dds = cars\\Xsara\\textures\\xsara_external.dds\r\n\
        xsara_external_scrape.dds = cars\\Xsara\\textures\\xsara_external_scrape.dds\r\n\
        [xsara_external.dds]\r\n\
        FriendlyName = \r\n\
        [xsara_external_scrape.dds]\r\n\
        FriendlyName = Scrape\r\n";

    fn sections(text: &str) -> Vec<Section> {
        ini::parse(text).unwrap()
    }

    #[test]
    fn reads_face_blocks_in_order() {
        let parts = sections(PART);
        let body = section(&parts, "E_BODY").unwrap();
        assert_eq!(
            face_blocks(body),
            vec![
                FaceBlock { shader: "CarPaint".to_owned(), material: "external::carpaint".to_owned() },
                FaceBlock { shader: "CarTexture".to_owned(), material: "external::tire".to_owned() },
            ]
        );
        assert!(!is_switched(body));
        assert!(is_switched(section(&parts, "e_light_lf").unwrap()));
    }

    #[test]
    fn part_without_face_blocks_has_none() {
        let parts = sections("[k_window_f]\r\nSwitch = false\r\n");
        assert!(face_blocks(&parts[0]).is_empty());
    }

    #[test]
    fn diffuse_path_follows_bitmap_and_texture_set() {
        let shaders = sections(SHADERS);
        assert_eq!(diffuse_path(&shaders, "external::carpaint"), Some("cars\\Xsara\\textures\\xsara_external.dds"));
        // The bitmap counts whatever the material's section says, as in the game's car shaders.
        assert_eq!(diffuse_path(&shaders, "external::tire"), Some("cars\\Xsara\\textures\\xsara_external.dds"));
        let only_bitmap = sections(&SHADERS.replace("[external::carpaint]\r\nDiffuseChannel = true\r\n", ""));
        assert_eq!(diffuse_path(&only_bitmap, "external::carpaint"), Some("cars\\Xsara\\textures\\xsara_external.dds"));
        assert_eq!(diffuse_path(&shaders, "external::missing"), None);
    }

    #[test]
    fn an_empty_texture_set_header_before_the_set_is_skipped() {
        let shaders = sections(&SHADERS.replace("[TSDefault]\r\n", "[TSDefault]\r\n[TSDefault]\r\n"));
        assert_eq!(diffuse_path(&shaders, "external::carpaint"), Some("cars\\Xsara\\textures\\xsara_external.dds"));
        assert_eq!(friendly_path(&shaders, "Scrape"), Some("cars\\Xsara\\textures\\xsara_external_scrape.dds"));
    }

    #[test]
    fn opacity_channel_is_read_per_material() {
        let shaders = sections(SHADERS);
        assert!(opacity_channel(&shaders, "external transparent::carpaint"));
        assert!(!opacity_channel(&shaders, "external::carpaint"));
        assert!(!opacity_channel(&shaders, "missing"));
    }

    #[test]
    fn friendly_path_finds_the_named_texture_in_the_set() {
        let shaders = sections(SHADERS);
        assert_eq!(friendly_path(&shaders, "Scrape"), Some("cars\\Xsara\\textures\\xsara_external_scrape.dds"));
        assert_eq!(friendly_path(&shaders, "InternalScrape"), None);
    }
}
