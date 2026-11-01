//! Persisted picker state. Stable stage numbers and car names survive catalogue rescans.
use rbr_formats::ini::Section;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub collection: Option<String>,
    pub query: String,
    pub sort: String,
    pub filter: String,
    pub length: String,
    pub selected: Option<String>,
    pub scroll: f32,
}

impl State {
    pub fn read(sections: &[Section], name: &str) -> Self {
        let Some(section) = sections.iter().find(|s| s.name == name) else { return Self::default() };
        let value = |key: &str| decode(section.get(key).unwrap_or_default());
        Self {
            collection: section.get("collection").map(decode),
            query: value("search"),
            sort: value("sort"),
            filter: value("filter"),
            length: value("length"),
            selected: section.get("selected").filter(|s| !s.is_empty()).map(decode),
            scroll: value("scroll").parse::<f32>().ok().filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(0.0),
        }
    }

    pub fn to_text(&self, name: &str) -> String {
        let Some(collection) = &self.collection else { return String::new() };
        let mut out = format!("\n[{name}]\n");
        for (key, value) in [
            ("collection", collection.clone()),
            ("search", self.query.clone()),
            ("sort", self.sort.clone()),
            ("filter", self.filter.clone()),
            ("length", self.length.clone()),
            ("selected", self.selected.clone().unwrap_or_default()),
            ("scroll", self.scroll.to_string()),
        ] {
            out.push_str(&format!("{key} = {}\n", encode(&value)));
        }
        out
    }
}

fn encode(value: &str) -> String {
    value.replace('%', "%25").replace('\n', "%0A").replace('\r', "%0D").replace(' ', "%20").replace('\t', "%09")
}

fn decode(value: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = value.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let mut look = bytes.clone();
            if let (Some(a), Some(b)) = (look.next(), look.next()) {
                if let Ok(n) = u8::from_str_radix(&String::from_utf8_lossy(&[a, b]), 16) {
                    out.push(n);
                    bytes = look;
                    continue;
                }
            }
        }
        out.push(b);
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_state_round_trips_text_and_rejects_invalid_scroll() {
        let state = State {
            collection: Some("class:Rally 2 / R5".into()),
            query: "  forêt 50%\n[next]".into(),
            sort: "recent".into(),
            filter: "4WD".into(),
            length: "any".into(),
            selected: Some("A car".into()),
            scroll: 123.5,
        };
        let sections = rbr_formats::ini::parse(&state.to_text("car_browser")).unwrap();
        assert_eq!(State::read(&sections, "car_browser"), state);
        assert_eq!(State::read(&sections, "stage_browser"), State::default());
        for scroll in ["NaN", "inf", "-4", "oops"] {
            let sections = rbr_formats::ini::parse(&format!("[car_browser]\nscroll = {scroll}\n")).unwrap();
            assert_eq!(State::read(&sections, "car_browser").scroll, 0.0);
        }
    }
}
