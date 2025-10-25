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

// Order is important. The lower the number, the earlier the part gets drawn.
// Because the header can overlap other parts, we draw it last.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PartId {
    Top = 0,
    Left = 1,
    Right = 2,
    Bottom = 3,
    Header = 4,
}

impl PartId {
    const TOP: usize = Self::Top as usize;
    const LEFT: usize = Self::Left as usize;
    const RIGHT: usize = Self::Right as usize;
    const BOTTOM: usize = Self::Bottom as usize;
    const HEADER: usize = Self::Header as usize;
    pub const COUNT: usize = 5;

    pub fn from_usize(v: usize) -> Self {
        match v {
            Self::TOP => PartId::Top,
            Self::LEFT => PartId::Left,
            Self::RIGHT => PartId::Right,
            Self::BOTTOM => PartId::Bottom,
            Self::HEADER => PartId::Header,
            _ => unreachable!(),
        }
    }
}

/// The decoration's 'parts'.
#[derive(Debug)]
pub struct DecorationParts {
    parts: [Part; PartId::COUNT],
    config: LayoutConfig,
}

impl DecorationParts {
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

        Self {
            parts,
            config: LayoutConfig::default(),
        }
    }

    pub fn parts(&self) -> impl Iterator<Item = (PartId, &Part)> {
        self.parts
            .iter()
            .enumerate()
            .map(|(id, part)| (PartId::from_usize(id), part))
    }

    fn parts_mut(&mut self) -> impl Iterator<Item = (PartId, &mut Part)> {
        self.parts
            .iter_mut()
            .enumerate()
            .map(|(id, part)| (PartId::from_usize(id), part))
    }

    /// Edge is a border + shadow
    fn edges_mut(&mut self) -> impl Iterator<Item = &mut Part> {
        self.parts_mut()
            .filter(|(idx, _)| *idx != PartId::Header)
            .map(|(_, p)| p)
    }

    pub fn header(&self) -> &Part {
        &self.parts[PartId::HEADER]
    }

    pub fn header_mut(&mut self) -> &mut Part {
        &mut self.parts[PartId::HEADER]
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

    pub fn relayout(&mut self, config: LayoutConfig) {
        if self.config == config {
            return;
        }
        self.config = config;

        let layout = PartLayout::calc(config);
        for (part, layout) in self.parts.iter_mut().zip(layout) {
            part.surface_rect = layout.surface_rect;
            part.input_rect = layout.input_rect;
        }
    }

    pub fn side_height(&self) -> u32 {
        self.parts[PartId::LEFT].surface_rect.height
    }

    pub fn find_surface(&self, surface: &ObjectId) -> Location {
        let found = self
            .parts()
            .find(|(_id, part)| &part.surface.id() == surface);

        let Some((id, _)) = found else {
            return Location::None;
        };

        match id {
            PartId::Header => Location::Head,
            PartId::Top => Location::Top,
            PartId::Bottom => Location::Bottom,
            PartId::Left => Location::Left,
            PartId::Right => Location::Right,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub struct LayoutConfig {
    pub width: u32,
    pub height: u32,
    pub hide_titlebar: bool,
    pub hide_border: bool,
    pub hide_edges: bool,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct PartLayout {
    /// Positioned relative to the main surface.
    pub surface_rect: Rect,
    /// Positioned relative to the local surface, aka. `surface_rect`.
    ///
    /// `None` if it fully covers `surface_rect`.
    pub input_rect: Option<Rect>,
}

impl PartLayout {
    pub fn calc(config: LayoutConfig) -> [Self; 5] {
        let LayoutConfig {
            width,
            height,
            hide_titlebar,
            hide_border,
            hide_edges,
        } = config;

        let mut parts = [Self::default(); 5];

        let header_size = if hide_titlebar { 0 } else { HEADER_SIZE };
        let height_with_header = height + header_size;

        let border_size = theme::border_size(hide_border);

        let width_with_border = width + 2 * border_size;
        let width_input_rect = width_with_border - (border_size * 2) + (RESIZE_HANDLE_SIZE * 2);

        let header_offset = header_size;

        parts[PartId::TOP].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32 + border_size as i32),
            width: width_with_border,
            height: border_size,
        };
        parts[PartId::TOP].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        parts[PartId::LEFT].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        parts[PartId::LEFT].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        parts[PartId::RIGHT].surface_rect = Rect {
            x: width as i32,
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        parts[PartId::RIGHT].input_rect = Some(Rect {
            x: 0,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        parts[PartId::BOTTOM].surface_rect = Rect {
            x: -(border_size as i32),
            y: height as i32,
            width: width_with_border,
            height: border_size,
        };
        parts[PartId::BOTTOM].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        parts[PartId::HEADER].surface_rect = Rect {
            x: 0,
            y: -(HEADER_SIZE as i32),
            width,
            height: HEADER_SIZE,
        };
        parts[PartId::HEADER].input_rect = None;

        let visible_border_size = theme::visible_border_size(hide_border);

        // XXX to perfectly align the visible borders we draw them with
        // the header, otherwise rounded corners won't look 'smooth' at the
        // start. To achieve that, we enlargen the width of the header by
        // 2 * `VISIBLE_BORDER_SIZE`, and move `x` by `VISIBLE_BORDER_SIZE`
        // to the left.
        if !hide_edges {
            parts[PartId::HEADER].surface_rect.width += 2 * visible_border_size;
            parts[PartId::HEADER].surface_rect.x -= visible_border_size as i32;
        }

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

        let mut layout = PartLayout::calc(LayoutConfig {
            width: 200,
            height: 200,
            hide_titlebar: false,
            hide_border: false,
            hide_edges: false,
        });

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                PartId::TOP => Color::from_rgba8(0, 0, 255, 255),
                PartId::LEFT => Color::from_rgba8(255, 0, 0, 255),
                PartId::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                PartId::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                PartId::HEADER => Color::from_rgba8(255, 255, 0, 255),
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

        let mut layout = PartLayout::calc(LayoutConfig {
            width: 200,
            height: 200,
            hide_titlebar: true,
            hide_border: false,
            hide_edges: false,
        });

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                PartId::TOP => Color::from_rgba8(0, 0, 255, 255),
                PartId::LEFT => Color::from_rgba8(255, 0, 0, 255),
                PartId::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                PartId::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                PartId::HEADER => continue,
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

        let mut layout = PartLayout::calc(LayoutConfig {
            width: 200,
            height: 200,
            hide_titlebar: false,
            hide_border: true,
            hide_edges: false,
        });

        for (part_idx, PartLayout { surface_rect, .. }) in layout.iter_mut().enumerate() {
            let color = match part_idx {
                PartId::TOP => Color::from_rgba8(0, 0, 255, 255),
                PartId::LEFT => Color::from_rgba8(255, 0, 0, 255),
                PartId::RIGHT => Color::from_rgba8(255, 0, 0, 255),
                PartId::BOTTOM => Color::from_rgba8(0, 0, 255, 255),
                PartId::HEADER => Color::from_rgba8(255, 255, 0, 255),
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
