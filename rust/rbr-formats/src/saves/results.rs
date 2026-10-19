//! The records table `SavedGames/pfresult.rst` (`CMenuSaving::WriteSave`, read `CMenuSaving::ReadFromBinStream`
//! 0x585c10; loaded by `CPC_SaveFileManager::Init` through 0x5859f0): 0x4000 bytes, not encrypted, checksummed,
//! `MENU`, 37 records of 0x34 bytes, `MENU`. The reader hands each record to `M00567330`, which stores it in the
//! gameflow's table (`M005653b0` gives them back for writing).

use super::{seal, unseal, Error, Writer};

pub const SIZE: usize = 0x4000;
pub const RECORDS: usize = 0x25;
const TAG: [u8; 4] = *b"UNEM";

#[derive(Clone, Debug, PartialEq)]
pub struct Results {
    pub records: Vec<ResultRecord>,
}

/// One record. The retail defaults hold "Richard Burns" times; the bytes after the name's NUL are kept as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResultRecord {
    pub id: u32,
    pub w04: u32,
    pub times: [f32; 3],
    pub name: [u8; 32],
}

impl ResultRecord {
    pub fn name(&self) -> String {
        super::season::c_str(&self.name)
    }
}

impl Results {
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = unseal(bytes, SIZE)?;
        r.tag(TAG)?;
        let count = r.u32()? as usize;
        if count > (SIZE - 16) / 0x34 {
            return Err(Error::TooLarge { len: count, size: RECORDS });
        }
        let records = (0..count)
            .map(|_| {
                Ok(ResultRecord {
                    id: r.u32()?,
                    w04: r.u32()?,
                    times: [r.f32()?, r.f32()?, r.f32()?],
                    name: r.array()?,
                })
            })
            .collect::<Result<_, Error>>()?;
        r.tag(TAG)?;
        Ok(Self { records })
    }

    pub fn write(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::default();
        w.u32(0);
        w.bytes(&TAG);
        w.u32(self.records.len() as u32);
        for rec in &self.records {
            w.words(&[rec.id, rec.w04]);
            rec.times.iter().for_each(|&t| w.f32(t));
            w.bytes(&rec.name);
        }
        w.bytes(&TAG);
        seal(w.out, SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_round_trip() {
        let mut name = [0xaa; 32];
        name[..4].copy_from_slice(b"Ann\0");
        let results = Results {
            records: (0..RECORDS as u32)
                .map(|k| ResultRecord { id: k + 10, w04: 5, times: [97.0, 194.0, 255.0], name })
                .collect(),
        };
        let bytes = results.write().unwrap();
        assert_eq!(bytes.len(), SIZE);
        let back = Results::read(&bytes).unwrap();
        assert_eq!(back, results);
        assert_eq!(back.records[0].name(), "Ann");
    }

    #[test]
    fn original_results_round_trip() {
        let Some(dir) = std::env::var_os("RBR_SAVES") else {
            eprintln!("skipping: RBR_SAVES not set");
            return;
        };
        let bytes = std::fs::read(std::path::Path::new(&dir).join("pfresult.rst")).unwrap();
        let results = Results::read(&bytes).unwrap();
        assert_eq!(results.records.len(), RECORDS);
        assert_eq!(results.records[0].name(), "Richard Burns");
        assert_eq!(results.write().unwrap(), bytes);
    }
}
