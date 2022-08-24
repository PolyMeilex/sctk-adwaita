//! Title renderer using ab_glyph & Cantarell-Regular.ttf (SIL Open Font Licence v1.1).
//!
//! Uses embedded font & requires no dynamically linked dependencies.
use ab_glyph::{point, Font, FontArc, Glyph, PxScale, ScaleFont};
use std::{
    fs::File,
    io::{BufReader, Read},
    process::Command,
};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");
/// Matches current crossfont version. TODO read system config size.
const DEFAULT_PT_SIZE: f32 = 10.0;

#[derive(Debug)]
pub struct AbGlyphTitleText {
    title: String,
    font: FontArc,
    size: PxScale,
    color: Color,
    pixmap: Option<Pixmap>,
}

impl AbGlyphTitleText {
    pub fn new(color: Color) -> Self {
        // Try to pick system default font
        let font = font_file_matching("sans-serif")
            .and_then(read_to_vec)
            .and_then(|data| FontArc::try_from_vec(data).ok())
            // fallback to using embedded font if system font doesn't work
            .unwrap_or_else(|| FontArc::try_from_slice(CANTARELL).unwrap());

        let size = font
            .pt_to_px_scale(DEFAULT_PT_SIZE)
            .expect("invalid font units_per_em");

        Self {
            title: <_>::default(),
            font,
            size,
            color,
            pixmap: None,
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        let new_scale = self
            .font
            .pt_to_px_scale(DEFAULT_PT_SIZE * scale as f32)
            .unwrap();
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
fn font_file_matching(pattern: &str) -> Option<File> {
    Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(pattern)
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
