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
        let header_size = if hide_titlebar { 0 } else { HEADER_SIZE };
        let height_with_header = height + header_size;

        let border_size = theme::border_size(hide_border);

        let width_with_border = width + 2 * border_size;
        let width_input_rect = width_with_border - (border_size * 2) + (RESIZE_HANDLE_SIZE * 2);

        let header_offset = header_size;

        self.parts[DecorationParts::TOP].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32 + border_size as i32),
            width: width_with_border,
            height: border_size,
        };
        self.parts[DecorationParts::TOP].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        self.parts[DecorationParts::LEFT].surface_rect = Rect {
            x: -(border_size as i32),
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        self.parts[DecorationParts::LEFT].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        self.parts[DecorationParts::RIGHT].surface_rect = Rect {
            x: width as i32,
            y: -(header_offset as i32),
            width: border_size,
            height: height_with_header,
        };
        self.parts[DecorationParts::RIGHT].input_rect = Some(Rect {
            x: 0,
            y: 0,
            width: RESIZE_HANDLE_SIZE,
            height: height_with_header,
        });

        self.parts[DecorationParts::BOTTOM].surface_rect = Rect {
            x: -(border_size as i32),
            y: height as i32,
            width: width_with_border,
            height: border_size,
        };
        self.parts[DecorationParts::BOTTOM].input_rect = Some(Rect {
            x: border_size as i32 - RESIZE_HANDLE_SIZE as i32,
            y: 0,
            width: width_input_rect,
            height: RESIZE_HANDLE_SIZE,
        });

        self.parts[DecorationParts::HEADER].surface_rect = Rect {
            x: 0,
            y: -(HEADER_SIZE as i32),
            width,
            height: HEADER_SIZE,
        };
        self.parts[DecorationParts::HEADER].input_rect = None;
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
