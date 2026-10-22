//! Whole-table checks against the previous, oracle-verified tables, represented only by hashes.
use rbr_formats::{data::Data, ngp};
use sha2::{Digest, Sha256};

fn word_hash(tables: impl IntoIterator<Item = ngp::Words>) -> String {
    let mut hash = Sha256::new();
    for mut words in tables {
        words.sort_unstable();
        for (offset, word) in words {
            hash.update(offset.to_le_bytes());
            hash.update(word.to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

#[test]
fn all_778_tables_match_the_oracle_verified_data() {
    let data = Data::from_env().unwrap_or_else(|| Data::new("", None));
    let Some(path) = ngp::path(&data) else {
        eprintln!("skipping: NGP.dll not configured (RBR_NGP_DLL or RBR_NGP)");
        return;
    };
    let tables = ngp::parse(&std::fs::read(&path).expect("NGP.dll")).expect("supported NGP.dll");
    assert_eq!(
        word_hash([tables.stage[0].clone()]),
        "d980585ebb6093405d9e3d7c6abbf40bae577db8971482714c50cba8ac5f7c51"
    );
    assert_eq!(
        word_hash([tables.stage[1].clone()]),
        "0d1eb553445c0d883a864aaf053f256186ce19648c25db3eaec249c9d8597919"
    );
    assert_eq!(
        word_hash(tables.tyres.iter().map(|t| t.definition.clone())),
        "6572fc1dca55c853617aed465cac82a6455fc6605c0dc68bce7553ddc0740a52"
    );
    assert_eq!(
        word_hash(tables.tyres.iter().map(|t| t.wheel.clone())),
        "305b48889407c39f5e775fc5eb72c2b1d9645a62c2c51d1ab10cd3a0ed8fc349"
    );
    let mut keys: Vec<_> =
        tables.keys.iter().map(|k| format!("key|{:?}|{}|{:08x}\n", k.class, k.name, k.offset)).collect();
    keys.extend(tables.removed.iter().map(|(c, k)| format!("removed|{c:?}|{k}\n")));
    keys.sort();
    assert_eq!(
        format!("{:x}", Sha256::digest(keys.concat().as_bytes())),
        "f07568e867aaeb029ae9a98a3f5892af57a31d61fd32a7726fc5fa31831db3a2"
    );
    let mut defaults = Sha256::new();
    let mut engine = tables.engine_defaults.clone();
    engine.sort_by_key(|&(offset, _)| offset);
    for (offset, value) in engine {
        defaults.update(offset.to_le_bytes());
        defaults.update(value.to_bits().to_le_bytes());
    }
    for value in tables
        .probe_defaults
        .iter()
        .chain(tables.temperatures.iter().flatten())
        .chain(&tables.tyre_ambient)
        .chain(&tables.temperature_offsets)
    {
        defaults.update(value.to_bits().to_le_bytes());
    }
    assert_eq!(
        format!("{:x}", defaults.finalize()),
        "4dc43d95bfb42e69611806e39725740166f117cb0ae8f24d5c061aaab7d58fb5"
    );
    assert_eq!(tables.road_surfaces.len(), 93);
    let roads: Vec<_> = tables.road_surfaces.iter().map(|&v| v as u8).collect();
    assert_eq!(
        format!("{:x}", Sha256::digest(&roads)),
        "0d716eca9db4e9067420090712dd8996b5cc14f84c551ecf7803f17bea3c8783"
    );
    assert!(tables.road_surfaces.contains(&254));
    assert!(!tables.road_surfaces.contains(&72));
}

#[test]
fn rejects_missing_truncated_and_unsupported_binaries() {
    for bytes in [&[][..], b"MZ", &[0; 512][..]] {
        assert!(ngp::parse(bytes).unwrap_err().contains("unsupported NGP.dll"));
    }
}
