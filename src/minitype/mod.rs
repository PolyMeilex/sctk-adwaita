mod font;
mod geometry;
mod outliner;

pub use font::Font;
pub use geometry::{point, vector, Point, Rect, Vector};

pub use ttf_parser::OutlineBuilder;

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u16);

impl From<ttf_parser::GlyphId> for GlyphId {
    fn from(id: ttf_parser::GlyphId) -> Self {
        Self(id.0)
    }
}
impl From<GlyphId> for ttf_parser::GlyphId {
    fn from(id: GlyphId) -> Self {
        Self(id.0)
    }
}

/// A single glyph of a font.
///
/// A `Glyph` does not have an inherent scale or position associated with it. To
/// augment a glyph with a size, give it a scale using `scaled`. You can then
/// position it using `positioned`.
#[derive(Clone, Debug)]
pub struct Glyph<'font> {
    font: Font<'font>,
    id: GlyphId,
}

impl<'font> Glyph<'font> {
    /// The font to which this glyph belongs.
    pub fn font(&self) -> &Font<'font> {
        &self.font
    }

    /// The glyph identifier for this glyph.
    pub fn id(&self) -> GlyphId {
        self.id
    }

    /// Augments this glyph with scaling information, making methods that depend
    /// on the scale of the glyph available.
    pub fn scaled(self, scale: Scale) -> ScaledGlyph<'font> {
        let scale_y = self.font.scale_for_pixel_height(scale.y);
        let scale_x = scale_y * scale.x / scale.y;
        ScaledGlyph {
            g: self,
            scale: vector(scale_x, scale_y),
        }
    }
}

/// The "horizontal metrics" of a glyph. This is useful for calculating the
/// horizontal offset of a glyph from the previous one in a string when laying a
/// string out horizontally.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct HMetrics {
    /// The horizontal offset that the origin of the next glyph should be from
    /// the origin of this glyph.
    pub advance_width: f32,
    /// The horizontal offset between the origin of this glyph and the leftmost
    /// edge/point of the glyph.
    pub left_side_bearing: f32,
}

/// The "vertical metrics" of a font at a particular scale. This is useful for
/// calculating the amount of vertical space to give a line of text, and for
/// computing the vertical offset between successive lines.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct VMetrics {
    /// The highest point that any glyph in the font extends to above the
    /// baseline. Typically positive.
    pub ascent: f32,
    /// The lowest point that any glyph in the font extends to below the
    /// baseline. Typically negative.
    pub descent: f32,
    /// The gap to leave between the descent of one line and the ascent of the
    /// next. This is of course only a guideline given by the font's designers.
    pub line_gap: f32,
}

impl core::ops::Mul<f32> for VMetrics {
    type Output = VMetrics;

    fn mul(self, rhs: f32) -> Self {
        Self {
            ascent: self.ascent * rhs,
            descent: self.descent * rhs,
            line_gap: self.line_gap * rhs,
        }
    }
}

/// A glyph augmented with scaling information. You can query such a glyph for
/// information that depends on the scale of the glyph.
#[derive(Clone, Debug)]
pub struct ScaledGlyph<'font> {
    g: Glyph<'font>,
    scale: Vector<f32>,
}

impl<'font> ScaledGlyph<'font> {
    /// The glyph identifier for this glyph.
    pub fn id(&self) -> GlyphId {
        self.g.id()
    }

    /// The font to which this glyph belongs.
    #[inline]
    pub fn font(&self) -> &Font<'font> {
        self.g.font()
    }

    /// Builds the outline of the glyph with the builder specified. Returns
    /// `false` when the outline is either malformed or empty.
    pub fn build_outline(&self, builder: &mut impl OutlineBuilder) -> bool {
        let mut outliner =
            outliner::OutlineScaler::new(builder, vector(self.scale.x, -self.scale.y));

        self.font()
            .inner()
            .outline_glyph(self.id().into(), &mut outliner)
            .is_some()
    }

    /// Augments this glyph with positioning information, making methods that
    /// depend on the position of the glyph available.
    pub fn positioned(self, p: Point<f32>) -> PositionedGlyph<'font> {
        let bb = self.pixel_bounds_at(p);
        PositionedGlyph {
            sg: self,
            position: p,
            bb,
        }
    }

    /// Retrieves the "horizontal metrics" of this glyph. See `HMetrics` for
    /// more detail.
    pub fn h_metrics(&self) -> HMetrics {
        let inner = self.font().inner();
        let id = self.id().into();

        let advance = inner.glyph_hor_advance(id).unwrap();
        let left_side_bearing = inner.glyph_hor_side_bearing(id).unwrap();

        HMetrics {
            advance_width: advance as f32 * self.scale.x,
            left_side_bearing: left_side_bearing as f32 * self.scale.x,
        }
    }

    fn glyph_bitmap_box_subpixel(
        &self,
        font: &Font<'font>,
        shift_x: f32,
        shift_y: f32,
    ) -> Option<Rect<i32>> {
        let ttf_parser::Rect {
            x_min,
            y_min,
            x_max,
            y_max,
        } = font.inner().glyph_bounding_box(self.id().into())?;

        Some(Rect {
            min: point(
                (x_min as f32 * self.scale.x + shift_x).floor() as i32,
                (-y_max as f32 * self.scale.y + shift_y).floor() as i32,
            ),
            max: point(
                (x_max as f32 * self.scale.x + shift_x).ceil() as i32,
                (-y_min as f32 * self.scale.y + shift_y).ceil() as i32,
            ),
        })
    }

    #[inline]
    fn pixel_bounds_at(&self, p: Point<f32>) -> Option<Rect<i32>> {
        // Use subpixel fraction in floor/ceil rounding to eliminate rounding error
        // from identical subpixel positions
        let (x_trunc, x_fract) = (p.x.trunc() as i32, p.x.fract());
        let (y_trunc, y_fract) = (p.y.trunc() as i32, p.y.fract());

        let Rect { min, max } = self.glyph_bitmap_box_subpixel(self.font(), x_fract, y_fract)?;
        Some(Rect {
            min: point(x_trunc + min.x, y_trunc + min.y),
            max: point(x_trunc + max.x, y_trunc + max.y),
        })
    }
}

/// A glyph augmented with positioning and scaling information. You can query
/// such a glyph for information that depends on the scale and position of the
/// glyph.
#[derive(Clone, Debug)]
pub struct PositionedGlyph<'font> {
    sg: ScaledGlyph<'font>,
    position: Point<f32>,
    bb: Option<Rect<i32>>,
}

impl<'font> PositionedGlyph<'font> {
    /// The glyph identifier for this glyph.
    pub fn id(&self) -> GlyphId {
        self.sg.id()
    }

    /// A reference to this glyph without positioning
    pub fn unpositioned(&self) -> &ScaledGlyph<'font> {
        &self.sg
    }

    /// The conservative pixel-boundary bounding box for this glyph. This is the
    /// smallest rectangle aligned to pixel boundaries that encloses the shape
    /// of this glyph at this position. Note that the origin of the glyph, at
    /// pixel-space coordinates (0, 0), is at the top left of the bounding box.
    pub fn pixel_bounding_box(&self) -> Option<Rect<i32>> {
        self.bb
    }

    pub fn position(&self) -> Point<f32> {
        self.position
    }

    /// Builds the outline of the glyph with the builder specified. Returns
    /// `false` when the outline is either malformed or empty.
    pub fn build_outline(&self, builder: &mut impl OutlineBuilder) -> bool {
        let bb = if let Some(bb) = self.bb.as_ref() {
            bb
        } else {
            return false;
        };

        let offset = vector(bb.min.x as f32, bb.min.y as f32);

        let mut outliner = outliner::OutlineTranslator::new(builder, self.position - offset);

        self.sg.build_outline(&mut outliner)
    }
}

/// Defines the size of a rendered face of a font, in pixels, horizontally and
/// vertically. A vertical scale of `y` pixels means that the distance between
/// the ascent and descent lines (see `VMetrics`) of the face will be `y`
/// pixels. If `x` and `y` are equal the scaling is uniform. Non-uniform scaling
/// by a factor *f* in the horizontal direction is achieved by setting `x` equal
/// to *f* times `y`.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Scale {
    /// Horizontal scale, in pixels.
    pub x: f32,
    /// Vertical scale, in pixels.
    pub y: f32,
}

impl Scale {
    /// Uniform scaling, equivalent to `Scale { x: s, y: s }`.
    #[inline]
    pub fn uniform(s: f32) -> Scale {
        Scale { x: s, y: s }
    }
}
/// A trait for types that can be converted into a `GlyphId`, in the context of
/// a specific font.
///
/// Many `rusttype` functions that operate on characters accept values of any
/// type that implements `IntoGlyphId`. Such types include `char`, `Codepoint`,
/// and obviously `GlyphId` itself.
pub trait IntoGlyphId {
    /// Convert `self` into a `GlyphId`, consulting the index map of `font` if
    /// necessary.
    fn into_glyph_id(self, font: &Font<'_>) -> GlyphId;
}
impl IntoGlyphId for char {
    #[inline]
    fn into_glyph_id(self, font: &Font<'_>) -> GlyphId {
        font.inner()
            .glyph_index(self)
            .unwrap_or(ttf_parser::GlyphId(0))
            .into()
    }
}
impl<G: Into<GlyphId>> IntoGlyphId for G {
    #[inline]
    fn into_glyph_id(self, _font: &Font<'_>) -> GlyphId {
        self.into()
    }
}

#[derive(Clone)]
pub struct LayoutIter<'a, 'font, 's> {
    font: &'a Font<'font>,
    chars: core::str::Chars<'s>,
    caret: f32,
    scale: Scale,
    start: Point<f32>,
    last_glyph: Option<GlyphId>,
}

impl<'a, 'font, 's> Iterator for LayoutIter<'a, 'font, 's> {
    type Item = PositionedGlyph<'font>;

    fn next(&mut self) -> Option<PositionedGlyph<'font>> {
        self.chars.next().map(|c| {
            let g = self.font.glyph(c).scaled(self.scale);
            if let Some(last) = self.last_glyph {
                self.caret += self.font.pair_kerning(self.scale, last, g.id());
            }
            let g = g.positioned(point(self.start.x + self.caret, self.start.y));
            self.caret += g.sg.h_metrics().advance_width;
            self.last_glyph = Some(g.id());
            g
        })
    }
}
