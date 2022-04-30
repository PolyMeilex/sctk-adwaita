use super::{Glyph, IntoGlyphId, LayoutIter, Point, Scale, VMetrics};
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct Font<'a>(Arc<ttf_parser::Face<'a>>);

impl fmt::Debug for Font<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Font")
    }
}

impl Font<'_> {
    /// Creates a Font from byte-slice data.
    ///
    /// Returns `None` for invalid data.
    pub fn try_from_bytes(bytes: &[u8]) -> Option<Font<'_>> {
        Self::try_from_bytes_and_index(bytes, 0)
    }

    /// Creates a Font from byte-slice data & a font collection `index`.
    ///
    /// Returns `None` for invalid data.
    pub fn try_from_bytes_and_index(bytes: &[u8], index: u32) -> Option<Font<'_>> {
        let inner = Arc::new(ttf_parser::Face::from_slice(bytes, index).ok()?);
        Some(Font(inner))
    }
}

impl<'font> Font<'font> {
    #[inline]
    pub(crate) fn inner(&self) -> &ttf_parser::Face<'_> {
        &self.0
    }

    /// The "vertical metrics" for this font at a given scale. These metrics are
    /// shared by all of the glyphs in the font. See `VMetrics` for more detail.
    pub fn v_metrics(&self, scale: Scale) -> VMetrics {
        self.v_metrics_unscaled() * self.scale_for_pixel_height(scale.y)
    }

    /// Get the unscaled VMetrics for this font, shared by all glyphs.
    /// See `VMetrics` for more detail.
    pub fn v_metrics_unscaled(&self) -> VMetrics {
        let font = self.inner();
        VMetrics {
            ascent: font.ascender() as f32,
            descent: font.descender() as f32,
            line_gap: font.line_gap() as f32,
        }
    }

    /// The number of glyphs present in this font. Glyph identifiers for this
    /// font will always be in the range `0..self.glyph_count()`
    pub fn glyph_count(&self) -> usize {
        self.inner().number_of_glyphs() as _
    }

    /// Returns the corresponding glyph for a Unicode code point or a glyph id
    /// for this font.
    pub fn glyph<C: IntoGlyphId>(&self, id: C) -> Glyph<'font> {
        let gid = id.into_glyph_id(self);
        assert!((gid.0 as usize) < self.glyph_count());
        // font clone either a reference clone, or arc clone
        Glyph {
            font: self.clone(),
            id: gid,
        }
    }

    /// A convenience function for laying out glyphs for a string horizontally.
    /// It does not take control characters like line breaks into account, as
    /// treatment of these is likely to depend on the application.
    pub fn layout<'a, 's>(
        &'a self,
        s: &'s str,
        scale: Scale,
        start: Point<f32>,
    ) -> LayoutIter<'a, 'font, 's> {
        LayoutIter {
            font: self,
            chars: s.chars(),
            caret: 0.0,
            scale,
            start,
            last_glyph: None,
        }
    }

    /// Returns additional kerning to apply as well as that given by HMetrics
    /// for a particular pair of glyphs.
    pub fn pair_kerning<A, B>(&self, scale: Scale, first: A, second: B) -> f32
    where
        A: IntoGlyphId,
        B: IntoGlyphId,
    {
        let first_id = first.into_glyph_id(self).into();
        let second_id = second.into_glyph_id(self).into();

        let factor = {
            let hscale = self.scale_for_pixel_height(scale.y);
            hscale * (scale.x / scale.y)
        };

        let kern = if let Some(kern) = self.inner().tables().kern {
            kern.subtables
                .into_iter()
                .filter(|st| st.horizontal && !st.variable)
                .find_map(|st| st.glyphs_kerning(first_id, second_id))
                .unwrap_or(0)
        } else {
            0
        };

        factor * f32::from(kern)
    }

    /// Computes a scale factor to produce a font whose "height" is 'pixels'
    /// tall. Height is measured as the distance from the highest ascender
    /// to the lowest descender; in other words, it's equivalent to calling
    /// GetFontVMetrics and computing:
    ///       scale = pixels / (ascent - descent)
    /// so if you prefer to measure height by the ascent only, use a similar
    /// calculation.
    pub fn scale_for_pixel_height(&self, height: f32) -> f32 {
        let inner = self.inner();
        let fheight = f32::from(inner.ascender()) - f32::from(inner.descender());
        height / fheight
    }
}
