use std::{
    iter::Enumerate,
    slice::{Iter, IterMut},
};

use smithay_client_toolkit::reexports::client::{
    backend::ObjectId,
    protocol::{wl_subsurface::WlSubsurface, wl_surface::WlSurface},
    Dispatch, Proxy, QueueHandle,
};

use smithay_client_toolkit::{
    compositor::SurfaceData,
    subcompositor::{SubcompositorState, SubsurfaceData},
};

use crate::theme::{self, HEADER_SIZE, RESIZE_HANDLE_SIZE};
use crate::{pointer::Location, wl_typed::WlTyped};

/// The decoration's 'parts'.
#[derive(Debug)]
pub struct DecorationParts {
    parts: [Part; 5],
}

impl DecorationParts {
    // Order is important. The lower the number, the earlier the part gets drawn.
    // Because the header can overlap other parts, we draw it last.
    pub const TOP: usize = 0;
    pub const LEFT: usize = 1;
    pub const RIGHT: usize = 2;
    pub const BOTTOM: usize = 3;
    pub const HEADER: usize = 4;

    pub fn new<State>(
        base_surface: &WlTyped<WlSurface, SurfaceData>,
        subcompositor: &SubcompositorState,
        queue_handle: &QueueHandle<State>,
    ) -> Self
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        let parts = [
            Part::new(base_surface, subcompositor, queue_handle),
            Part::new(base_surface, subcompositor, queue_handle),
            Part::new(base_surface, subcompositor, queue_handle),
            Part::new(base_surface, subcompositor, queue_handle),
            Part::new(base_surface, subcompositor, queue_handle),
        ];

        Self { parts }
    }

    pub fn parts(&self) -> Enumerate<Iter<'_, Part>> {
        self.parts.iter().enumerate()
    }

    pub fn parts_mut(&mut self) -> Enumerate<IterMut<'_, Part>> {
        self.parts.iter_mut().enumerate()
    }

    /// Edge is a border + shadow
    fn edges_mut(&mut self) -> impl Iterator<Item = &mut Part> {
        self.parts_mut()
            .filter(|(idx, _)| *idx != Self::HEADER)
            .map(|(_, p)| p)
    }

    pub fn header(&self) -> &Part {
        &self.parts[Self::HEADER]
    }

    pub fn header_mut(&mut self) -> &mut Part {
        &mut self.parts[Self::HEADER]
    }

    pub fn hide(&mut self) {
        for part in self.parts.iter_mut() {
            part.hide = true;
            part.subsurface.set_sync();
            part.surface.attach(None, 0, 0);
            part.surface.commit();
        }
    }

    pub fn show(&mut self) {
        for part in self.parts.iter_mut() {
            part.hide = false;
        }
    }

    /// Edge is a border + shadow
    pub fn hide_edges(&mut self) {
        for part in self.edges_mut() {
            part.hide = true;
            part.surface.attach(None, 0, 0);
            part.surface.commit();
        }
    }

    pub fn hide_titlebar(&mut self) {
        let part = self.header_mut();
        part.hide = true;
        part.surface.attach(None, 0, 0);
        part.surface.commit();
    }

    pub fn resize(&mut self, width: u32, height: u32, hide_titlebar: bool, hide_border: bool) {
        let layout = PartLayout::calc(width, height, hide_titlebar, hide_border);
        for (part, layout) in self.parts.iter_mut().zip(layout) {
            part.surface_rect = layout.surface_rect;
            part.input_rect = layout.input_rect;
        }
    }

    pub fn side_height(&self) -> u32 {
        self.parts[Self::LEFT].surface_rect.height
    }

    pub fn find_surface(&self, surface: &ObjectId) -> Location {
        let pos = match self
            .parts
            .iter()
            .position(|part| &part.surface.id() == surface)
        {
            Some(pos) => pos,
            None => return Location::None,
        };

        match pos {
            Self::HEADER => Location::Head,
            Self::TOP => Location::Top,
            Self::BOTTOM => Location::Bottom,
            Self::LEFT => Location::Left,
            Self::RIGHT => Location::Right,
            _ => unreachable!(),
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct PartLayout {
    /// Positioned relative to the main surface.
    surface_rect: Rect,
    /// Positioned relative to the local surface, aka. `surface_rect`.
    ///
    /// `None` if it fully covers `surface_rect`.
    input_rect: Option<Rect>,
}

impl PartLayout {
    fn calc(width: u32, height: u32, hide_titlebar: bool, hide_border: bool) -> [Self; 5] {
        let mut parts = [Self::default(); 5];

        let header_size = if hide_titlebar { 0 } else { HEADER_SIZE };
        let height_with_header = height + header_size;

        let border_size = theme::border_size(hide_border);

        let width_with_border = width + 2 * border_size;
        let width_input_rect = width_with_border - (border_size * 2) + (RESIZE_HANDLE_SIZE * 2);

        let header_offset = header_size;

        parts[DecorationParts::TOP].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32 + border_size as i32),
            width: width_with_border,
            height: border_size,
        };
        parts[DecorationParts::TOP].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        parts[DecorationParts::LEFT].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        parts[DecorationParts::LEFT].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        parts[DecorationParts::RIGHT].surface_rect = Rect {
            x: width as i32,
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        parts[DecorationParts::RIGHT].input_rect = Some(Rect {
            x: 0,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        parts[DecorationParts::BOTTOM].surface_rect = Rect {
            x: -(border_size as i32),
            y: height as i32,
            width: width_with_border,
            height: border_size,
        };
        parts[DecorationParts::BOTTOM].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        parts[DecorationParts::HEADER].surface_rect = Rect {
            x: 0,
            y: -(HEADER_SIZE as i32),
            width,
            height: HEADER_SIZE,
        };
        parts[DecorationParts::HEADER].input_rect = None;

        parts
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct Part {
    pub surface: WlTyped<WlSurface, SurfaceData>,
    pub subsurface: WlTyped<WlSubsurface, SubsurfaceData>,

    /// Positioned relative to the main surface.
    pub surface_rect: Rect,
    /// Positioned relative to the local surface, aka. `surface_rect`.
    ///
    /// `None` if it fully covers `surface_rect`.
    pub input_rect: Option<Rect>,

    pub hide: bool,
}

impl Part {
    fn new<State>(
        parent: &WlTyped<WlSurface, SurfaceData>,
        subcompositor: &SubcompositorState,
        queue_handle: &QueueHandle<State>,
    ) -> Part
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        let (subsurface, surface) =
            subcompositor.create_subsurface(parent.inner().clone(), queue_handle);

        let subsurface = WlTyped::wrap::<State>(subsurface);
        let surface = WlTyped::wrap::<State>(surface);

        // Sync with the parent surface.
        subsurface.set_sync();

        Part {
            surface,
            subsurface,
            surface_rect: Rect::default(),
            input_rect: None,
            hide: false,
        }
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tiny_skia::{Color, Paint, Pixmap, Shader, Transform};

    use super::*;

    fn expected_file_path(name: &str) -> String {
        format!("./tests/subsurface-layout/{name}.expected.png")
    }
    fn got_file_path(name: &str) -> String {
        format!("./tests/subsurface-layout/{name}.got.png")
    }

    #[track_caller]
    fn png_check(name: &str, got: &[u8]) {
        let expected = std::fs::read(expected_file_path(name)).unwrap();
        std::fs::write(got_file_path(name), got).unwrap();
        assert_eq!(
            expected,
            got,
            "Mismatch in the file: {}",
            got_file_path(name)
        );
    }

    #[allow(unused)]
    #[track_caller]
    fn png_update_expected(name: &str, got: &[u8]) {
        std::fs::write(expected_file_path(name), got).unwrap();
    }

    #[test]
    fn layout() {
        let mut pixmap = Pixmap::new(400, 400).unwrap();
        pixmap.fill(Color::WHITE);

        pixmap.fill_rect(
            tiny_skia::Rect::from_xywh(100.0, 100.0, 200.0, 200.0).unwrap(),
            &Paint {
                shader: Shader::SolidColor(Color::BLACK),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );

        let mut layout = PartLayout::calc(200, 200, false, false);

        let visible_border_size = theme::visible_border_size(false);

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                DecorationParts::TOP => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::LEFT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::HEADER => {
                    // TODO: Why is this not a part of the layout calc?
                    surface_rect.width += 2 * visible_border_size;
                    surface_rect.x -= visible_border_size as i32;

                    Color::from_rgba8(255, 255, 0, 255)
                }
                _ => unreachable!(),
            };

            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(
                    surface_rect.x as f32 + 100.0,
                    surface_rect.y as f32 + 100.0,
                    surface_rect.width as f32,
                    surface_rect.height as f32,
                )
                .unwrap(),
                &Paint {
                    shader: Shader::SolidColor(color),
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }

        let got = pixmap.encode_png().unwrap();
        png_check("layout", &got);
    }

    #[test]
    fn layout_no_titlebar() {
        let mut pixmap = Pixmap::new(400, 400).unwrap();
        pixmap.fill(Color::WHITE);

        pixmap.fill_rect(
            tiny_skia::Rect::from_xywh(100.0, 100.0, 200.0, 200.0).unwrap(),
            &Paint {
                shader: Shader::SolidColor(Color::BLACK),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );

        let mut layout = PartLayout::calc(200, 200, true, false);

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                DecorationParts::TOP => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::LEFT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::HEADER => continue,
                _ => unreachable!(),
            };

            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(
                    surface_rect.x as f32 + 100.0,
                    surface_rect.y as f32 + 100.0,
                    surface_rect.width as f32,
                    surface_rect.height as f32,
                )
                .unwrap(),
                &Paint {
                    shader: Shader::SolidColor(color),
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }

        let got = pixmap.encode_png().unwrap();
        png_check("layout-no-titlebar", &got);
    }

    #[test]
    fn layout_no_border() {
        let mut pixmap = Pixmap::new(400, 400).unwrap();
        pixmap.fill(Color::WHITE);

        pixmap.fill_rect(
            tiny_skia::Rect::from_xywh(100.0, 100.0, 200.0, 200.0).unwrap(),
            &Paint {
                shader: Shader::SolidColor(Color::BLACK),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );

        let hide_border = true;

        let mut layout = PartLayout::calc(200, 200, false, hide_border);

        let visible_border_size = theme::visible_border_size(hide_border);

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                DecorationParts::TOP => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::LEFT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                DecorationParts::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                DecorationParts::HEADER => {
                    // TODO: Why is this not a part of the layout calc?
                    surface_rect.width += 2 * visible_border_size;
                    surface_rect.x -= visible_border_size as i32;

                    Color::from_rgba8(255, 255, 0, 255)
                }
                _ => unreachable!(),
            };

            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(
                    surface_rect.x as f32 + 100.0,
                    surface_rect.y as f32 + 100.0,
                    surface_rect.width as f32,
                    surface_rect.height as f32,
                )
                .unwrap(),
                &Paint {
                    shader: Shader::SolidColor(color),
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }

        let got = pixmap.encode_png().unwrap();
        png_check("layout-no-border", &got);
    }
}
