//! The `key = value` settings files the original reads through `NLBLib2::CConfig` (`PC_InputFilter.ini` in the game
//! folder). Sections and keys match without case; blank lines and lines starting with `;` or `#` are skipped (the
//! original's comment rule is not traced, and the game's files have none). The typed getters follow the original's:
//! `N2_411670` takes `true` or `false` (`_stricmp`) and the default otherwise; `N2_4115b0` takes a float (`sscanf`
//! `%f`, here the whole trimmed value) and the default when it does not parse.

/// A parsed settings file: `(section, key, value)` in file order, with the section and key lower-cased.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Ini {
    entries: Vec<(String, String, String)>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let mut section = String::new();
        let mut entries = Vec::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                section = name.trim().to_ascii_lowercase();
            } else if let Some((key, value)) = line.split_once('=') {
                entries.push((section.clone(), key.trim().to_ascii_lowercase(), value.trim().to_string()));
            }
        }
        Self { entries }
    }

    /// The first value of the key in the section, if the file has one.
    fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (section, key) = (section.to_ascii_lowercase(), key.to_ascii_lowercase());
        self.entries.iter().find(|(s, k, _)| *s == section && *k == key).map(|(_, _, value)| value.as_str())
    }

    /// `N2_411670`: `true` or `false` without case, else `default`.
    pub fn bool(&self, section: &str, key: &str, default: bool) -> bool {
        match self.get(section, key) {
            Some(v) if v.eq_ignore_ascii_case("false") => false,
            Some(v) if v.eq_ignore_ascii_case("true") => true,
            _ => default,
        }
    }

    /// `N2_4115b0`: the value as a float, else `default`.
    pub fn float(&self, section: &str, key: &str, default: f32) -> f32 {
        self.get(section, key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_keys_and_values_without_case() {
        let ini = Ini::parse("; c\n[STEER::AXIS]\nactive = FALSE\nmaxSpeedFade=0.3\n\n[steer::axis]\nx = 1\n");
        assert!(!ini.bool("STEER::AXIS", "Active", true));
        assert_eq!(ini.float("STEER::AXIS", "MaxSpeedFade", 9.0), 0.3);
        assert_eq!(ini.float("STEER::AXIS", "x", 9.0), 1.0);
    }

    #[test]
    fn missing_or_odd_values_take_the_default() {
        let ini = Ini::parse("[S]\nb = yes\nf = fast\n");
        assert!(ini.bool("S", "b", true));
        assert!(!ini.bool("S", "missing", false));
        assert_eq!(ini.float("S", "f", 2.5), 2.5);
        assert_eq!(ini.float("T", "f", 2.5), 2.5);
    }
}
