//! Title renderer using ab_glyph & Cantarell-Regular.ttf (SIL Open Font Licence v1.1).
//!
//! Uses embedded font & requires no dynamically linked dependencies.
use crate::title::{config, font_preference::FontPreference};
use ab_glyph::{point, Font, FontArc, FontVec, Glyph, PxScale, ScaleFont, VariableFont};
use std::{
    fs::File,
    io::{BufReader, Read},
    process::Command,
};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct AbGlyphTitleText {
    title: String,
    font: FontArc,
    original_px_size: f32,
    size: PxScale,
    color: Color,
    pixmap: Option<Pixmap>,
}

impl AbGlyphTitleText {
    pub fn new(color: Color) -> Self {
        let font_pref = config::titlebar_font().unwrap_or_default();
        let font = font_file_matching(&font_pref)
            .and_then(read_to_vec)
            .and_then(|data| {
                let mut font = FontVec::try_from_vec(data).ok()?;
                // basic "bold" handling for variable fonts
                if font_pref
                    .style
                    .map_or(false, |s| s.eq_ignore_ascii_case("bold"))
                {
                    font.set_variation(b"wght", 700.0);
                }
                Some(FontArc::from(font))
            })
            // fallback to using embedded font if system font doesn't work
            .unwrap_or_else(|| FontArc::try_from_slice(CANTARELL).unwrap());

        let size = font
            .pt_to_px_scale(font_pref.pt_size)
            .expect("invalid font units_per_em");

        Self {
            title: <_>::default(),
            font,
            original_px_size: size.x,
            size,
            color,
            pixmap: None,
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        let new_scale = PxScale::from(self.original_px_size * scale as f32);
        if (self.size.x - new_scale.x).abs() > f32::EPSILON {
            self.size = new_scale;
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
        let font = self.font.as_scaled(self.size);

        let glyphs = self.layout();
        let last_glyph = glyphs.last()?;
        let width = (last_glyph.position.x + font.h_advance(last_glyph.id)).ceil() as u32;
        let height = font.height().ceil() as u32;

        let mut pixmap = Pixmap::new(width, height)?;

        let pixels = pixmap.pixels_mut();

        for glyph in glyphs {
            if let Some(outline) = self.font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                let left = bounds.min.x as u32;
                let top = bounds.min.y as u32;
                outline.draw(|x, y, c| {
                    let p_idx = (top + y) * width + (left + x);
                    let old_alpha_u8 = pixels[p_idx as usize].alpha();
                    let new_alpha = c + (old_alpha_u8 as f32 / 255.0);
                    if let Some(px) = PremultipliedColorU8::from_rgba(
                        (self.color.red() * new_alpha * 255.0) as _,
                        (self.color.green() * new_alpha * 255.0) as _,
                        (self.color.blue() * new_alpha * 255.0) as _,
                        (new_alpha * 255.0) as _,
                    ) {
                        pixels[p_idx as usize] = px;
                    }
                })
            }
        }

        Some(pixmap)
    }

    /// Simple single-line glyph layout.
    fn layout(&self) -> Vec<Glyph> {
        let font = self.font.as_scaled(self.size);

        let mut caret = point(0.0, font.ascent());
        let mut last_glyph: Option<Glyph> = None;
        let mut target = Vec::new();
        for c in self.title.chars() {
            if c.is_control() {
                continue;
            }
            let mut glyph = font.scaled_glyph(c);
            if let Some(previous) = last_glyph.take() {
                caret.x += font.kern(previous.id, glyph.id);
            }
            glyph.position = caret;

            last_glyph = Some(glyph.clone());
            caret.x += font.h_advance(glyph.id);

            target.push(glyph);
        }
        target
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

fn read_to_vec(file: File) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    BufReader::new(file).read_to_end(&mut data).ok()?;
    Some(data)
}
