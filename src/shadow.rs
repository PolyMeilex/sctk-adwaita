use crate::{parts::PartId, theme};
use std::collections::{btree_map, BTreeMap};
use tiny_skia::{Pixmap, PixmapMut, PixmapRef, Point, PremultipliedColorU8};

// These values were generated from a screenshot of an libadwaita window using a script.
// For more details see: https://github.com/PolyMeilex/sctk-adwaita/pull/43
pub const SHADOW_SIZE: u32 = 43;
const SHADOW_PARAMS_ACTIVE: (f32, f32, f32) = (0.206_505_5, 0.104_617_53, -0.000_542_446_2);
const SHADOW_PARAMS_INACTIVE: (f32, f32, f32) = (0.168_297_29, 0.204_299_8, 0.001_769_798_6);

fn shadow(pixel_dist: f32, scale: u32, active: bool) -> f32 {
    let (a, b, c) = if active {
        SHADOW_PARAMS_ACTIVE
    } else {
        SHADOW_PARAMS_INACTIVE
    };

    a * (-b * (pixel_dist / scale as f32)).exp() + c
}

#[derive(Debug)]
struct RenderedShadow {
    side: Pixmap,
    edges: Pixmap,
}

impl RenderedShadow {
    fn new(scale: u32, active: bool) -> Option<RenderedShadow> {
        let shadow_size = SHADOW_SIZE * scale;
        let corner_radius = theme::CORNER_RADIUS * scale;

        let mut side = Pixmap::new(shadow_size, 1)?;
        for x in 0..side.width() as usize {
            let shadow = shadow(x as f32 + 0.5, scale, active);
            let alpha = (shadow * u8::MAX as f32).round() as u8;

            let Some(color) = PremultipliedColorU8::from_rgba(0, 0, 0, alpha) else {
                continue;
            };

            if let Some(pixel) = side.pixels_mut().get_mut(x) {
                *pixel = color;
            }
        }

        let edges_size = (corner_radius + shadow_size) * 2;
        let mut edges = Pixmap::new(edges_size, edges_size)?;
        let edges_middle = Point::from_xy(edges_size as f32 / 2.0, edges_size as f32 / 2.0);
        for y in 0..edges_size as usize {
            let y_pos = y as f32 + 0.5;
            for x in 0..edges_size as usize {
                let dist = edges_middle.distance(Point::from_xy(x as f32 + 0.5, y_pos))
                    - corner_radius as f32;

                let shadow = shadow(dist, scale, active);
                let alpha = (shadow * u8::MAX as f32).round() as u8;

                let Some(color) = PremultipliedColorU8::from_rgba(0, 0, 0, alpha) else {
                    continue;
                };

                if let Some(pixel) = edges.pixels_mut().get_mut(y * edges_size as usize + x) {
                    *pixel = color;
                }
            }
        }

        Some(RenderedShadow { side, edges })
    }

    fn side_draw(
        &self,
        flipped: bool,
        rotated: bool,
        stack: usize,
        dst_pixmap: &mut PixmapMut,
        dst_left: usize,
        dst_top: usize,
    ) {
        fn iter_copy<'a>(
            src: impl Iterator<Item = &'a PremultipliedColorU8>,
            dst: impl Iterator<Item = &'a mut PremultipliedColorU8>,
        ) {
            src.zip(dst).for_each(|(src, dst)| *dst = *src)
        }

        let dst_width = dst_pixmap.width() as usize;
        let dst_pixels = dst_pixmap.pixels_mut();
        match (flipped, rotated) {
            (false, false) => (0..stack).for_each(|i| {
                let dst = dst_pixels
                    .iter_mut()
                    .skip((dst_top + i) * dst_width + dst_left);
                iter_copy(self.side.pixels().iter(), dst);
            }),
            (false, true) => (0..stack).for_each(|i| {
                let dst = dst_pixels
                    .iter_mut()
                    .skip(dst_top * dst_width + dst_left + i)
                    .step_by(dst_width);
                iter_copy(self.side.pixels().iter(), dst);
            }),
            (true, false) => (0..stack).for_each(|i| {
                let dst = dst_pixels
                    .iter_mut()
                    .skip((dst_top + i) * dst_width + dst_left);
                iter_copy(self.side.pixels().iter().rev(), dst);
            }),
            (true, true) => (0..stack).for_each(|i| {
                let dst = dst_pixels
                    .iter_mut()
                    .skip(dst_top * dst_width + dst_left + i)
                    .step_by(dst_width);
                iter_copy(self.side.pixels().iter().rev(), dst);
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn edges_draw(
        &self,
        src_x_offset: isize,
        src_y_offset: isize,
        dst_pixmap: &mut PixmapMut,
        dst_rect_left: usize,
        dst_rect_top: usize,
        dst_rect_width: usize,
        dst_rect_height: usize,
    ) {
        let src_width = self.edges.width() as usize;
        let src_pixels = self.edges.pixels();
        let dst_width = dst_pixmap.width() as usize;
        let dst_pixels = dst_pixmap.pixels_mut();
        for y in 0..dst_rect_height {
            let dst_y = dst_rect_top + y;
            let src_y = y as isize + src_y_offset;
            if src_y < 0 {
                continue;
            }

            let src_y = src_y as usize;
            for x in 0..dst_rect_width {
                let dst_x = dst_rect_left + x;
                let src_x = x as isize + src_x_offset;
                if src_x < 0 {
                    continue;
                }

                let src = src_pixels.get(src_y * src_width + src_x as usize);
                let dst = dst_pixels.get_mut(dst_y * dst_width + dst_x);
                if let (Some(src), Some(dst)) = (src, dst) {
                    *dst = *src;
                }
            }
        }
    }

    fn draw(&self, dst_pixmap: &mut PixmapMut, scale: u32, part_idx: PartId, hide_border: bool) {
        let shadow_size = (SHADOW_SIZE * scale) as usize;
        let visible_border_size = (theme::visible_border_size(hide_border) * scale) as usize;
        let corner_radius = (theme::CORNER_RADIUS * scale) as usize;

        debug_assert!(corner_radius > visible_border_size);
        if corner_radius <= visible_border_size {
            return;
        }

        let dst_width = dst_pixmap.width() as usize;
        let dst_height = dst_pixmap.height() as usize;
        let edges_half = self.edges.width() as usize / 2;
        match part_idx {
            PartId::Top => {
                let left_edge_width = edges_half;
                let right_edge_width = edges_half;
                let side_width = dst_width
                    .saturating_sub(left_edge_width)
                    .saturating_sub(right_edge_width);

                self.edges_draw(
                    0,
                    -(visible_border_size as isize),
                    dst_pixmap,
                    0,
                    0,
                    left_edge_width,
                    dst_height,
                );

                self.side_draw(
                    true,
                    true,
                    side_width,
                    dst_pixmap,
                    left_edge_width,
                    visible_border_size,
                );

                self.edges_draw(
                    edges_half as isize,
                    -(visible_border_size as isize),
                    dst_pixmap,
                    left_edge_width + side_width,
                    0,
                    right_edge_width,
                    dst_height,
                );
            }
            PartId::Left => {
                let top_edge_height = corner_radius;
                let bottom_edge_height = corner_radius - visible_border_size;
                let side_height = dst_height
                    .saturating_sub(top_edge_height)
                    .saturating_sub(bottom_edge_height);

                self.edges_draw(
                    0,
                    shadow_size as isize,
                    dst_pixmap,
                    0,
                    0,
                    dst_width.saturating_sub(visible_border_size),
                    top_edge_height,
                );

                self.side_draw(true, false, side_height, dst_pixmap, 0, top_edge_height);

                self.edges_draw(
                    0,
                    edges_half as isize,
                    dst_pixmap,
                    0,
                    top_edge_height + side_height,
                    dst_width.saturating_sub(visible_border_size),
                    bottom_edge_height,
                );
            }
            PartId::Right => {
                let top_edge_height = corner_radius;
                let bottom_edge_height = corner_radius - visible_border_size;
                let side_height = dst_height
                    .saturating_sub(top_edge_height)
                    .saturating_sub(bottom_edge_height);

                self.edges_draw(
                    edges_half as isize + corner_radius as isize,
                    shadow_size as isize,
                    dst_pixmap,
                    visible_border_size,
                    0,
                    dst_width.saturating_sub(visible_border_size),
                    top_edge_height,
                );

                self.side_draw(
                    false,
                    false,
                    side_height,
                    dst_pixmap,
                    visible_border_size,
                    top_edge_height,
                );

                self.edges_draw(
                    edges_half as isize + corner_radius as isize,
                    edges_half as isize,
                    dst_pixmap,
                    visible_border_size,
                    top_edge_height + side_height,
                    dst_width.saturating_sub(visible_border_size),
                    bottom_edge_height,
                );
            }
            PartId::Bottom => {
                let left_edge_width = edges_half;
                let right_edge_width = edges_half;
                let side_width = dst_width
                    .saturating_sub(left_edge_width)
                    .saturating_sub(right_edge_width);

                self.edges_draw(
                    0,
                    edges_half as isize + (corner_radius - visible_border_size) as isize,
                    dst_pixmap,
                    0,
                    0,
                    left_edge_width,
                    dst_height,
                );

                self.side_draw(
                    false,
                    true,
                    side_width,
                    dst_pixmap,
                    left_edge_width,
                    visible_border_size,
                );

                self.edges_draw(
                    edges_half as isize,
                    edges_half as isize + (corner_radius - visible_border_size) as isize,
                    dst_pixmap,
                    left_edge_width + side_width,
                    0,
                    right_edge_width,
                    dst_height,
                );
            }
            PartId::Header => {
                self.edges_draw(
                    shadow_size as isize,
                    shadow_size as isize,
                    dst_pixmap,
                    0,
                    0,
                    corner_radius,
                    corner_radius,
                );

                self.edges_draw(
                    edges_half as isize,
                    shadow_size as isize,
                    dst_pixmap,
                    dst_width.saturating_sub(corner_radius),
                    0,
                    corner_radius,
                    corner_radius,
                );
            }
        }
    }
}

#[derive(Debug)]
struct CachedPart {
    pixmap: Pixmap,
    scale: u32,
    active: bool,
    hide_border: bool,
}

impl CachedPart {
    fn matches(
        &self,
        dst_pixmap: &PixmapRef,
        dst_scale: u32,
        dst_active: bool,
        hide_border: bool,
    ) -> bool {
        self.pixmap.width() == dst_pixmap.width()
            && self.pixmap.height() == dst_pixmap.height()
            && self.scale == dst_scale
            && self.active == dst_active
            && self.hide_border == hide_border
    }
}

#[derive(Default, Debug)]
pub struct Shadow {
    part_cache: [Option<CachedPart>; PartId::COUNT],
    // (scale, active) -> RenderedShadow
    rendered: BTreeMap<(u32, bool), RenderedShadow>,
}

impl Shadow {
    pub fn draw(
        &mut self,
        pixmap: &mut PixmapMut,
        scale: u32,
        active: bool,
        part_idx: PartId,
        hide_border: bool,
    ) {
        let Some(cache) = self.part_cache.get_mut(part_idx as usize) else {
            return;
        };

        if let Some(cache_value) = cache {
            if !cache_value.matches(&pixmap.as_ref(), scale, active, hide_border) {
                *cache = None;
            }
        }

        if cache.is_none() {
            let rendered = match self.rendered.entry((scale, active)) {
                btree_map::Entry::Occupied(entry) => entry.into_mut(),
                btree_map::Entry::Vacant(entry) => match RenderedShadow::new(scale, active) {
                    Some(v) => entry.insert(v),
                    None => return,
                },
            };

            let Some(mut pixmap) = Pixmap::new(pixmap.width(), pixmap.height()) else {
                return;
            };

            rendered.draw(&mut pixmap.as_mut(), scale, part_idx, hide_border);

            *cache = Some(CachedPart {
                pixmap,
                scale,
                active,
                hide_border,
            })
        }

        if let Some(cache) = cache.as_ref() {
            let src_data = cache.pixmap.data();
            if let Some(pixmap) = pixmap.data_mut().get_mut(..src_data.len()) {
                pixmap.copy_from_slice(src_data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

    use tiny_skia::Color;

    use super::*;

    fn expected_file_path(name: &str) -> String {
        format!("./tests/shadow/{name}.expected.png")
    }
    fn got_file_path(name: &str) -> String {
        format!("./tests/shadow/{name}.got.png")
    }

    #[track_caller]
    fn png_check(name: &str, got: &[u8]) {
        let expected = std::fs::read(expected_file_path(name)).unwrap();
        std::fs::write(got_file_path(name), got).unwrap();
        if expected != got {
            panic!("Mismatch in the file: {}", got_file_path(name));
        }
    }

    #[allow(unused)]
    #[track_caller]
    fn png_update_expected(name: &str, got: &[u8]) {
        std::fs::write(expected_file_path(name), got).unwrap();
    }

    #[test]
    fn source_texture_side_active() {
        let shadow = RenderedShadow::new(1, true).unwrap();
        let got = shadow.side.encode_png().unwrap();
        png_check("side-single-row-active", &got);
    }

    #[test]
    fn source_texture_side_inactive() {
        let shadow = RenderedShadow::new(1, false).unwrap();
        let got = shadow.side.encode_png().unwrap();
        png_check("side-single-row-inactive", &got);
    }

    #[test]
    fn source_texture_corner_active() {
        let shadow = RenderedShadow::new(1, true).unwrap();
        let got = shadow.edges.encode_png().unwrap();
        png_check("corner-source-active", &got);
    }

    #[test]
    fn source_texture_corner_inactive() {
        let shadow = RenderedShadow::new(1, false).unwrap();
        let got = shadow.edges.encode_png().unwrap();
        png_check("corner-source-inactive", &got);
    }

    #[test]
    fn side_left() {
        let mut pixmap = Pixmap::new(60, 100).unwrap();
        pixmap.fill(Color::WHITE);

        let shadow = RenderedShadow::new(1, true).unwrap();

        shadow.side_draw(
            false,
            false,
            pixmap.height() as usize,
            &mut pixmap.as_mut(),
            0,
            0,
        );

        let got = pixmap.encode_png().unwrap();
        png_check("side-left", &got);
    }

    #[test]
    fn side_left_flipped() {
        let mut pixmap = Pixmap::new(60, 100).unwrap();
        pixmap.fill(Color::WHITE);

        let shadow = RenderedShadow::new(1, true).unwrap();

        shadow.side_draw(
            true,
            false,
            pixmap.height() as usize,
            &mut pixmap.as_mut(),
            0,
            0,
        );

        let got = pixmap.encode_png().unwrap();
        png_check("side-left-flipped", &got);
    }

    #[test]
    fn side_left_rotated() {
        let mut pixmap = Pixmap::new(60, 100).unwrap();
        pixmap.fill(Color::WHITE);

        let shadow = RenderedShadow::new(1, true).unwrap();

        shadow.side_draw(
            false,
            true,
            pixmap.width() as usize,
            &mut pixmap.as_mut(),
            0,
            0,
        );

        let got = pixmap.encode_png().unwrap();
        png_check("side-left-rotated", &got);
    }

    #[test]
    fn side_left_flipped_rotated() {
        let mut pixmap = Pixmap::new(60, 100).unwrap();
        pixmap.fill(Color::WHITE);

        let shadow = RenderedShadow::new(1, true).unwrap();

        shadow.side_draw(
            true,
            true,
            pixmap.width() as usize,
            &mut pixmap.as_mut(),
            0,
            0,
        );

        let got = pixmap.encode_png().unwrap();
        png_check("side-left-flipped-rotated", &got);
    }
}
