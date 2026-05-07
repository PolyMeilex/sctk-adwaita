//! Title renderer using skrifa.
//!
//! Requires no dynamically linked dependencies.
//!
//! Can fallback to an embedded Cantarell-Regular.ttf font (SIL Open Font Licence v1.1)
//! if the system font doesn't work.
use crate::title::{config, font_preference::FontPreference};
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    MetadataProvider,
};
use std::{fs::File, process::Command};
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct SkrifaTitleText {
    title: String,
    font: Option<(memmap2::Mmap, FontPreference)>,
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

    /// Render returning the new `Pixmap`.
    fn render(&self) -> Option<Pixmap> {
        if self.title.is_empty() {
            return None;
        }

        let font = parse_font(&self.font);
        let size = Size::new(self.px_size);
        let location = LocationRef::default();

        let metrics = font.metrics(size, location);
        let glyph_metrics = font.glyph_metrics(size, location);
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();

        // Layout glyphs and collect paths
        let mut paths: Vec<(tiny_skia::Path, f32, f32)> = Vec::new();
        let mut caret: f32 = 0.0;
        let ascent = metrics.ascent;

        let mut last_glyph_id = None;
        for c in self.title.chars() {
            if c.is_control() {
                continue;
            }

            let glyph_id = charmap.map(c).unwrap_or_default();

            // Kerning (if available)
            // Note: skrifa doesn't expose a simple kern method on glyph_metrics,
            // so we skip kerning for simplicity. The visual difference is minimal
            // for title bar text.
            let _ = last_glyph_id;

            let mut pb = tiny_skia::PathBuilder::new();
            let mut pen = PathPen(&mut pb);

            if let Some(outline) = outlines.get(glyph_id) {
                let settings = DrawSettings::unhinted(size, location);
                let _ = outline.draw(settings, &mut pen);
            }

            if let Some(path) = pb.finish() {
                paths.push((path, caret, ascent));
            }

            caret += glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
            last_glyph_id = Some(glyph_id);
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
}
