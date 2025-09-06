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

use crate::theme::{BORDER_SIZE, HEADER_SIZE, RESIZE_HANDLE_SIZE};
use crate::{pointer::Location, wl_typed::WlTyped};

/// The decoration's 'parts'.
#[derive(Debug)]
pub struct DecorationParts {
    parts: [Part; 5],
    hide_titlebar: bool,
}

impl DecorationParts {
    // XXX keep in sync with `Self;:new`.
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
        hide_titlebar: bool,
    ) -> Self
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        let header_offset = if hide_titlebar { 0 } else { HEADER_SIZE };

        // XXX the order must be in sync with associated constants.
        let parts = [
            // Top.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                Rect {
                    x: -(BORDER_SIZE as i32),
                    y: -(header_offset as i32 + BORDER_SIZE as i32),
                    width: 0, // Defined by `Self::resize`.
                    height: BORDER_SIZE,
                },
                Some(Rect {
                    x: BORDER_SIZE as i32 - RESIZE_HANDLE_SIZE as i32,
                    y: BORDER_SIZE as i32 - RESIZE_HANDLE_SIZE as i32,
                    width: 0, // Defined by `Self::resize`.
                    height: RESIZE_HANDLE_SIZE,
                }),
            ),
            // Left.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                Rect {
                    x: -(BORDER_SIZE as i32),
                    y: -(header_offset as i32),
                    width: BORDER_SIZE,
                    height: 0, // Defined by `Self::resize`.
                },
                Some(Rect {
                    x: BORDER_SIZE as i32 - RESIZE_HANDLE_SIZE as i32,
                    y: 0,
                    width: RESIZE_HANDLE_SIZE,
                    height: 0, // Defined by `Self::resize`.
                }),
            ),
            // Right.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                Rect {
                    x: 0, // Defined by `Self::resize`.
                    y: -(header_offset as i32),
                    width: BORDER_SIZE,
                    height: 0, // Defined by `Self::resize`.
                },
                Some(Rect {
                    x: 0,
                    y: 0,
                    width: RESIZE_HANDLE_SIZE,
                    height: 0, // Defined by `Self::resize`.
                }),
            ),
            // Bottom.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                Rect {
                    x: -(BORDER_SIZE as i32),
                    y: 0,     // Defined by `Self::resize`.
                    width: 0, // Defined by `Self::resize`.
                    height: BORDER_SIZE,
                },
                Some(Rect {
                    x: BORDER_SIZE as i32 - RESIZE_HANDLE_SIZE as i32,
                    y: 0,
                    width: 0, // Defined by `Self::resize`,
                    height: RESIZE_HANDLE_SIZE,
                }),
            ),
            // Header.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                Rect {
                    x: 0,
                    y: -(HEADER_SIZE as i32),
                    width: 0, // Defined by `Self::resize`.
                    height: HEADER_SIZE,
                },
                None,
            ),
        ];

        Self {
            parts,
            hide_titlebar,
        }
    }

    pub fn parts(&self) -> Enumerate<Iter<'_, Part>> {
        self.parts.iter().enumerate()
    }

    pub fn parts_mut(&mut self) -> Enumerate<IterMut<'_, Part>> {
        self.parts.iter_mut().enumerate()
    }

    pub fn borders_mut(&mut self) -> impl Iterator<Item = &mut Part> {
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

    pub fn hide_borders(&mut self) {
        for part in self.borders_mut() {
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

    // These unwraps are guaranteed to succeed because the affected options are filled above
    // and then never emptied afterwards.
    pub fn resize(&mut self, width: u32, height: u32) {
        let header_size = if self.hide_titlebar { 0 } else { HEADER_SIZE };
        let height_with_header = height + header_size;

        let width_with_border = width + 2 * BORDER_SIZE;
        let width_input_rect = width_with_border - (BORDER_SIZE * 2) + (RESIZE_HANDLE_SIZE * 2);

        self.parts[Self::HEADER].surface_rect.width = width;

        self.parts[Self::BOTTOM].surface_rect.width = width_with_border;
        self.parts[Self::BOTTOM].surface_rect.y = height as i32;
        if let Some(input_rect) = self.parts[Self::BOTTOM].input_rect.as_mut() {
            input_rect.width = width_input_rect;
        }

        self.parts[Self::TOP].surface_rect.width = width_with_border;
        if let Some(input_rect) = self.parts[Self::TOP].input_rect.as_mut() {
            input_rect.width = width_input_rect;
        }

        self.parts[Self::LEFT].surface_rect.height = height_with_header;
        if let Some(input_rect) = self.parts[Self::LEFT].input_rect.as_mut() {
            input_rect.height = height_with_header;
        }

        self.parts[Self::RIGHT].surface_rect.height = height_with_header;
        self.parts[Self::RIGHT].surface_rect.x = width as i32;
        if let Some(input_rect) = self.parts[Self::RIGHT].input_rect.as_mut() {
            input_rect.height = height_with_header;
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

#[derive(Debug, Clone, Copy)]
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
        surface_rect: Rect,
        input_rect: Option<Rect>,
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
            surface_rect,
            input_rect,
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
