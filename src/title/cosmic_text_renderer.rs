use crate::title::{config, font_preference::FontPreference};
use cosmic_text::{
    Attrs, AttrsList, BufferLine, Color as CosmicColor, Family, FontSystem, Metrics, Shaping,
    SwashCache, Wrap,
};
use tiny_skia::{Color, ColorU8, Pixmap, PremultipliedColorU8};

// Use arbitrarily large width, then calculate max/min x from resulting glyphs
const MAX_WIDTH: f32 = 1024.0 * 1024.0;

fn attrs_from_font_pref(font_preference: &FontPreference) -> Attrs {
    let attrs = Attrs::new().family(Family::Name(&font_preference.name));
    if let Some(style) = &font_preference.style {
        // TODO
    }
    attrs
}

pub struct CosmicTextTitleText {
    buffer_line: BufferLine,
    cache: SwashCache,
    color: Color,
    scale: u32,
    pixmap: Option<Pixmap>,
    font_pref: FontPreference,
    font_system: FontSystem,
}

impl std::fmt::Debug for CosmicTextTitleText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TitleText")
            .field("color", &self.color)
            .field("scale", &self.scale)
            .field("pixmap", &self.pixmap)
            .field("font_pref", &self.font_pref)
            .finish()
    }
}

impl CosmicTextTitleText {
    pub fn new(color: Color) -> Self {
        let buffer_line = BufferLine::new("", AttrsList::new(Attrs::new()), Shaping::Advanced);
        let cache = SwashCache::new();
        let font_pref = config::titlebar_font().unwrap_or_default();
        Self {
            buffer_line,
            cache,
            color,
            scale: 1,
            pixmap: None,
            font_pref,
            font_system: FontSystem::new(),
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        self.scale = scale;
        self.update_pixmap();
    }

    pub fn update_title<S: Into<String>>(&mut self, title: S) {
        let attrs = attrs_from_font_pref(&self.font_pref);
        self.buffer_line
            .set_text(title.into(), AttrsList::new(attrs));
        self.update_pixmap();
    }

    pub fn update_color(&mut self, color: Color) {
        self.color = color;
        self.update_pixmap();
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    fn update_pixmap(&mut self) {
        self.pixmap = None;

        let shape_line = self.buffer_line.shape(&mut self.font_system);

        let height = (1.4 * self.scale as f32 * self.font_pref.pt_size * 96.0 / 72.0).ceil() as i32; // ?
        let layout_lines = shape_line.layout(height as f32, MAX_WIDTH, Wrap::Word, None);
        let layout_line = &layout_lines[0];
        let min_x = layout_line
            .glyphs
            .iter()
            .map(|i| i.x.floor() as u32)
            .min()
            .unwrap_or(0);
        let width = layout_line.w.ceil() as u32;

        let mut pixmap = match Pixmap::new(width, height as u32) {
            Some(pixmap) => pixmap,
            None => return,
        };
        let pixels = pixmap.pixels_mut();

        let color = self.color.to_color_u8();
        let color = CosmicColor::rgba(color.red(), color.green(), color.blue(), color.alpha());
        for glyph in &layout_line.glyphs {
            let physical_glyph = glyph.physical((0., 0.), 1.); // TODO scale
            self.cache.with_pixels(
                &mut self.font_system,
                physical_glyph.cache_key,
                color,
                |pixel_x, pixel_y, color| {
                    if color.a() == 0 {
                        return;
                    }

                    let y = height + physical_glyph.y + pixel_y;
                    let x = physical_glyph.x + pixel_x - min_x as i32;
                    let idx = y * width as i32 + x;
                    if idx >= 0 && (idx as usize) < pixels.len() {
                        let idx = idx as usize;
                        let color = ColorU8::from_rgba(color.r(), color.g(), color.b(), color.a());
                        pixels[idx] = alpha_blend(color.premultiply(), pixels[idx]);
                    }
                },
            );
        }

        self.pixmap = Some(pixmap);
    }
}

// `a` over `b`
// This should be correct but not especially efficient.
fn alpha_blend(a: PremultipliedColorU8, b: PremultipliedColorU8) -> PremultipliedColorU8 {
    let blend_channel = |channel_a: u8, channel_b: u8| -> u8 {
        channel_a.saturating_add(channel_b.saturating_mul(255 - a.alpha()))
    };
    PremultipliedColorU8::from_rgba(
        blend_channel(a.red(), b.red()),
        blend_channel(a.green(), b.green()),
        blend_channel(a.blue(), b.blue()),
        blend_channel(a.alpha(), b.alpha()),
    )
    .unwrap()
}
