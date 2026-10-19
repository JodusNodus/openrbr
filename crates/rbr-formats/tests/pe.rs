//! The retail 1.02 exe's sections, read through `pe`. Runs only when `RBR_DATA` and `RBR_PATCH` are set (the patch
//! holds the 1.02 exe); checks words the load path relies on.

use rbr_formats::{data::Data, pe};

#[test]
fn retail_exe_maps_the_vtables_the_load_reads() {
    let Some(path) =
        Data::from_env().filter(|d| d.patch().is_some()).and_then(|d| d.resolve("RichardBurnsRally_SSE.exe"))
    else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let img = pe::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(img.base, 0x400000);
    // Slot 2 (Parse) of CSpringDamper::CDef, NTweak::CReal and NTweak::CUnsigned.
    assert_eq!(img.u32(0x740124 + 8), Some(0x52f0c0));
    assert_eq!(img.u32(0x73b644 + 8), Some(0x52f770));
    assert_eq!(img.u32(0x740534 + 8), Some(0x52f840));
    // The "SpringDamper" name the definition's constructor stores.
    assert_eq!(img.read(0x740174, 13), Some(&b"SpringDamper\0"[..]));
    assert!(img.section(".data").is_some() && img.section(".rdata").is_some());
}
