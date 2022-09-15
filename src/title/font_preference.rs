#[derive(Debug)]
pub(crate) struct FontPreference {
    pub name: String,
    pub style: Option<String>,
    pub pt_size: f32,
}

impl Default for FontPreference {
    fn default() -> Self {
        Self {
            name: "sans-serif".into(),
            style: None,
            pt_size: 10.0,
        }
    }
}

impl FontPreference {
    /// Parse config string like `Cantarell 12` or `Cantarell Bold 11`.
    pub fn from_name_style_size(conf: &str) -> Option<Self> {
        let mut split = conf.split(' ');
        let name = split.next()?;
        let mut style = split.next();
        let mut pt_size = split.next();
        if let (Some(v), None) = (style, pt_size) {
            if v.chars().all(|c| c.is_numeric() || c == '.') {
                pt_size = Some(v);
                style = None;
            }
        }

        let pt_size = pt_size.and_then(|p| p.parse().ok()).unwrap_or(10.0);

        Some(Self {
            name: name.into(),
            style: style.map(|v| v.into()),
            pt_size,
        })
    }
}

#[test]
fn pref_from_name_variant_size() {
    let pref = FontPreference::from_name_style_size("Cantarell Bold 12").unwrap();
    assert_eq!(pref.name, "Cantarell");
    assert_eq!(pref.style, Some("Bold".into()));
    assert!((pref.pt_size - 12.0).abs() < f32::EPSILON);
}

#[test]
fn pref_from_name_size() {
    let pref = FontPreference::from_name_style_size("Cantarell 12").unwrap();
    assert_eq!(pref.name, "Cantarell");
    assert_eq!(pref.style, None);
    assert!((pref.pt_size - 12.0).abs() < f32::EPSILON);
}

#[test]
fn pref_from_name() {
    let pref = FontPreference::from_name_style_size("Cantarell").unwrap();
    assert_eq!(pref.name, "Cantarell");
    assert_eq!(pref.style, None);
    assert!((pref.pt_size - 10.0).abs() < f32::EPSILON);
}
