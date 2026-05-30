//! Title renderer using skrifa.
//!
//! Requires no dynamically linked dependencies.
//!
//! Can fallback to an embedded Cantarell-Regular.ttf font (SIL Open Font Licence v1.1)
//! if the system font doesn't work.
//!
//! Per-codepoint fallback faces (emoji, CJK, …) are discovered via `fc-match`;
//! see [`crate::title::font_fallback`].
use crate::title::{config, font_fallback::FallbackCache, font_preference::FontPreference};
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, HintingInstance, HintingOptions, OutlinePen},
    GlyphId, MetadataProvider,
};
use std::{fs::File, process::Command};
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct SkrifaTitleText {
    title: String,
    font: Option<(memmap2::Mmap, FontPreference)>,
    fallbacks: FallbackCache,
    original_px_size: f32,
    px_size: f32,
    color: Color,
    pixmap: Option<Pixmap>,
}

impl SkrifaTitleText {
    pub fn new(color: Color) -> Self {
        let font_pref = config::titlebar_font().unwrap_or_default();
        let font_pref_pt_size = font_pref.pt_size;
        let font = font_file_matching(&font_pref)
            .and_then(|f| mmap(&f))
            .map(|mmap| (mmap, font_pref));

        let px_size = pt_to_px(&font, font_pref_pt_size);

        Self {
            title: <_>::default(),
            font,
            fallbacks: FallbackCache::default(),
            original_px_size: px_size,
            px_size,
            color,
            pixmap: None,
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        let new_size = self.original_px_size * scale as f32;
        if (self.px_size - new_size).abs() > f32::EPSILON {
            self.px_size = new_size;
            self.pixmap = self.render();
        }
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        let new_title = title.into();
        if new_title != self.title {
            self.title = new_title;
            self.discover_fallbacks();
            self.pixmap = self.render();
        }
    }

    pub fn update_color(&mut self, color: Color) {
        if color != self.color {
            self.color = color;
            self.pixmap = self.render();
        }
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    /// Load additional faces for any title codepoint not covered by the primary
    /// font or any previously-loaded fallback.
    fn discover_fallbacks(&mut self) {
        let primary = parse_font(&self.font);
        self.fallbacks.extend(&self.title, |c, loaded| {
            covers(&primary, c)
                || loaded
                    .iter()
                    .any(|m| skrifa::FontRef::from_index(m, 0).is_ok_and(|fr| covers(&fr, c)))
        });
    }

    /// Render returning the new `Pixmap`.
    fn render(&self) -> Option<Pixmap> {
        if self.title.is_empty() {
            return None;
        }

        let primary = parse_font(&self.font);
        let fallbacks: Vec<_> = self
            .fallbacks
            .fonts
            .iter()
            .filter_map(|m| skrifa::FontRef::from_index(m, 0).ok())
            .collect();

        let size = Size::new(self.px_size);
        let location = LocationRef::default();
        let metrics = primary.metrics(size, location);

        // Per-font caches: metrics, charmap, outlines, hinting. Hinting falls
        // back to unhinted rendering if the font doesn't support it.
        let primary_face = Face::new(&primary, size, location);
        let fallback_faces: Vec<Face<'_>> = fallbacks
            .iter()
            .map(|f| Face::new(f, size, location))
            .collect();

        // Layout: pick the first face (primary, then fallbacks) that covers each char.
        let mut paths: Vec<(tiny_skia::Path, f32, f32)> = Vec::new();
        let mut caret: f32 = 0.0;
        let ascent = metrics.ascent;

        for c in self.title.chars() {
            if c.is_control() {
                continue;
            }

            let face = select_face(&primary_face, &fallback_faces, c);
            let glyph_id = face.charmap_get(c).unwrap_or_default();

            let mut pb = tiny_skia::PathBuilder::new();
            let mut pen = PathPen(&mut pb);

            if let Some(outline) = face.outlines.get(glyph_id) {
                let settings = match &face.hinting {
                    Some(h) => DrawSettings::from(h),
                    None => DrawSettings::unhinted(size, location),
                };
                let _ = outline.draw(settings, &mut pen);
            }

            if let Some(path) = pb.finish() {
                paths.push((path, caret, ascent));
            }

            caret += face.glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
        }

        if paths.is_empty() {
            return None;
        }

        // Calculate bounding box
        let total_width = caret.ceil() as u32;
        let total_height = (metrics.ascent - metrics.descent).ceil() as u32;

        if total_width == 0 || total_height == 0 {
            return None;
        }

        // Render using tiny-skia
        let mut pixmap = Pixmap::new(total_width, total_height)?;
        let mut paint = Paint::default();
        paint.set_color(self.color);
        paint.anti_alias = true;

        for (path, x, y) in &paths {
            let transform = Transform::from_translate(*x, *y);
            pixmap.fill_path(path, &paint, FillRule::Winding, transform, None);
        }

        Some(pixmap)
    }
}

/// One parsed face + per-size caches reused across the line.
struct Face<'a> {
    charmap: skrifa::charmap::Charmap<'a>,
    outlines: skrifa::outline::OutlineGlyphCollection<'a>,
    glyph_metrics: skrifa::metrics::GlyphMetrics<'a>,
    hinting: Option<HintingInstance>,
}

impl<'a> Face<'a> {
    fn new(font: &skrifa::FontRef<'a>, size: Size, location: LocationRef<'a>) -> Self {
        let outlines = font.outline_glyphs();
        let hinting =
            HintingInstance::new(&outlines, size, location, HintingOptions::default()).ok();
        Self {
            charmap: font.charmap(),
            glyph_metrics: font.glyph_metrics(size, location),
            outlines,
            hinting,
        }
    }

    fn charmap_get(&self, c: char) -> Option<GlyphId> {
        let id = self.charmap.map(c)?;
        // Treat `.notdef` (id 0) as no coverage so we look further down the stack.
        if id == GlyphId::NOTDEF {
            None
        } else {
            Some(id)
        }
    }
}

fn covers(font: &skrifa::FontRef<'_>, c: char) -> bool {
    match font.charmap().map(c) {
        Some(id) => id != GlyphId::NOTDEF,
        None => false,
    }
}

/// Pick the first face that covers `c`, falling back to the primary face
/// (which will draw `.notdef`) when no face has it.
fn select_face<'a, 'b>(
    primary: &'a Face<'b>,
    fallbacks: &'a [Face<'b>],
    c: char,
) -> &'a Face<'b> {
    if primary.charmap_get(c).is_some() {
        return primary;
    }
    fallbacks
        .iter()
        .find(|f| f.charmap_get(c).is_some())
        .unwrap_or(primary)
}

/// Convert point size to pixel size using font's units_per_em.
fn pt_to_px(font_data: &Option<(memmap2::Mmap, FontPreference)>, pt_size: f32) -> f32 {
    let font = parse_font(font_data);
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let units_per_em = metrics.units_per_em as f32;
    if units_per_em > 0.0 {
        // Standard conversion: 1pt = 1.333px at 96dpi (96/72)
        pt_size * (96.0 / 72.0)
    } else {
        // Fallback
        pt_size * 1.333
    }
}

/// Parse the memmapped system font or fallback to built-in Cantarell.
fn parse_font<'a>(sys_font: &'a Option<(memmap2::Mmap, FontPreference)>) -> skrifa::FontRef<'a> {
    match sys_font {
        Some((mmap, _font_pref)) => skrifa::FontRef::from_index(mmap, 0).unwrap_or_else(|_| {
            #[allow(clippy::unwrap_used)]
            skrifa::FontRef::from_index(CANTARELL, 0).unwrap()
        }),
        #[allow(clippy::unwrap_used)]
        _ => skrifa::FontRef::from_index(CANTARELL, 0).unwrap(),
    }
}

/// Font-config without dynamically linked dependencies
fn font_file_matching(pref: &FontPreference) -> Option<File> {
    let mut pattern = pref.name.clone();
    if let Some(style) = &pref.style {
        pattern.push(':');
        pattern.push_str(style);
    }
    Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(&pattern)
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|path| File::open(path.trim()).ok())
}

fn mmap(file: &File) -> Option<memmap2::Mmap> {
    // Safety: System font files are not expected to be mutated during use
    unsafe { memmap2::Mmap::map(file).ok() }
}

/// A pen that draws skrifa glyph outlines into a `tiny_skia::PathBuilder`.
struct PathPen<'a>(&'a mut tiny_skia::PathBuilder);

impl OutlinePen for PathPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, -cy0, x, -y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }

    fn close(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Helper to create a renderer using only the bundled Cantarell font.
    fn new_renderer() -> SkrifaTitleText {
        let mut renderer = SkrifaTitleText {
            title: String::new(),
            font: None, // forces fallback to bundled Cantarell
            fallbacks: FallbackCache::default(),
            original_px_size: 13.3,
            px_size: 13.3,
            color: Color::BLACK,
            pixmap: None,
        };
        renderer.update_title("Hello");
        renderer
    }

    #[test]
    fn renders_non_empty_pixmap() {
        let renderer = new_renderer();
        let pixmap = renderer.pixmap().expect("should produce a pixmap");
        assert!(pixmap.width() > 0);
        assert!(pixmap.height() > 0);
    }

    #[test]
    fn renders_pixels_with_coverage() {
        let renderer = new_renderer();
        let pixmap = renderer.pixmap().expect("should produce a pixmap");
        // At least some pixels should have non-zero alpha (i.e. glyphs were drawn)
        let has_visible = pixmap.pixels().iter().any(|px| px.alpha() > 0);
        assert!(has_visible, "rendered text should have visible pixels");
    }

    #[test]
    fn empty_title_returns_none() {
        let mut renderer = new_renderer();
        renderer.update_title("");
        assert!(renderer.pixmap().is_none());
    }

    #[test]
    fn update_scale_changes_size() {
        let mut renderer = new_renderer();
        let w1 = renderer.pixmap().expect("pixmap").width();
        renderer.update_scale(2);
        let w2 = renderer.pixmap().expect("pixmap after scale").width();
        assert!(w2 > w1, "scaled text should be wider");
    }

    #[test]
    fn update_color_rerenders() {
        let mut renderer = new_renderer();
        let old_pixels: Vec<_> = renderer.pixmap().unwrap().pixels().to_vec();
        renderer.update_color(Color::from_rgba8(255, 0, 0, 255));
        let new_pixels: Vec<_> = renderer.pixmap().unwrap().pixels().to_vec();
        assert_ne!(
            old_pixels, new_pixels,
            "color change should produce different pixels"
        );
    }

    #[test]
    fn parse_bundled_font_succeeds() {
        let font = parse_font(&None);
        let metrics = font.metrics(Size::new(16.0), LocationRef::default());
        assert!(metrics.units_per_em > 0);
        assert!(metrics.ascent > 0.0);
    }

    /// Regression: '★' is not in Cantarell. Before this fix it silently
    /// rendered as Cantarell's `.notdef` glyph. The fix must load a fallback
    /// face that has it.
    #[test]
    fn loads_fallback_for_missing_codepoint() {
        let mut renderer = new_renderer();
        renderer.update_title("★");
        assert!(
            !renderer.fallbacks.fonts.is_empty(),
            "'★' is not in Cantarell — a fallback face must have been loaded"
        );
    }

    /// Pure-ASCII titles must not trigger any fallback lookups.
    #[test]
    fn no_fallback_for_ascii_title() {
        let renderer = new_renderer();
        assert!(
            renderer.fallbacks.fonts.is_empty(),
            "ASCII fits in Cantarell, no fallback should be loaded"
        );
    }
}
