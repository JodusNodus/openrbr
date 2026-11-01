//! Native stage introductions documented in RSF's dated announcements. A map fix never changes this date.
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    New,
    Conversion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    pub date: &'static str,
    pub kind: Kind,
}

impl Release {
    pub fn label(self) -> &'static str {
        match self.kind {
            Kind::New => "Added to RSF",
            Kind::Conversion => "Native conversion",
        }
    }
}

const DATA: &str = include_str!("../../../data/rsf-stage-releases.csv");
static RELEASES: LazyLock<BTreeMap<u32, Release>> = LazyLock::new(|| {
    DATA.lines()
        .filter_map(|line| {
            let mut fields = line.split(',');
            let id = fields.next()?.parse().ok()?;
            let date = fields.next()?;
            let kind = match fields.next()? {
                "new" => Kind::New,
                "conversion" => Kind::Conversion,
                _ => return None,
            };
            Some((id, Release { date, kind }))
        })
        .collect()
});

pub fn get(id: u32) -> Option<Release> {
    RELEASES.get(&id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_dates_cover_new_maps_and_conversions_without_bumping_updates() {
        let rows: Vec<_> = DATA.lines().filter(|l| !l.starts_with('#')).skip(1).collect();
        assert_eq!(rows.len(), RELEASES.len(), "invalid or duplicate release entries");
        for release in RELEASES.values() {
            let date = release.date;
            assert_eq!(date.len(), 10);
            assert_eq!(&date[4..5], "-");
            assert_eq!(&date[7..8], "-");
            assert!((1..=12).contains(&date[5..7].parse::<u32>().unwrap()));
            assert!((1..=31).contains(&date[8..].parse::<u32>().unwrap()));
        }
        for id in [609, 610, 621, 629, 630, 631, 634, 635, 641] {
            assert_eq!(get(id), Some(Release { date: "2026-08-15", kind: Kind::New }));
        }
        assert_eq!(get(626), Some(Release { date: "2026-08-15", kind: Kind::Conversion }));
        assert_eq!(get(604).unwrap().date, "2026-04-03"); // August collision fixes did not introduce Tahekeroa.
        assert_eq!(get(564).unwrap().date, "2025-04-21"); // El Condor had later updates too.
        assert_eq!(get(999999), None);
    }
}
