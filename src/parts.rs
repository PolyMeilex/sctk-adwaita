use smithay_client_toolkit::reexports::client::{
    protocol::{wl_subsurface::WlSubsurface, wl_surface::WlSurface},
    Dispatch, Proxy, QueueHandle,
};

use smithay_client_toolkit::{
    compositor::SurfaceData,
    subcompositor::{SubcompositorState, SubsurfaceData},
};

use crate::pointer::Location;
use crate::theme::{BORDER_SIZE, HEADER_SIZE};

/// The decoration's 'parts'.
#[derive(Debug)]
pub struct DecorationParts {
    parts: [Part; 5],
}

impl DecorationParts {
    // XXX keep in sync with `Self;:new`.
    pub const HEADER: usize = 0;
    pub const TOP: usize = 1;
    pub const LEFT: usize = 2;
    pub const RIGHT: usize = 3;
    pub const BOTTOM: usize = 4;

    pub fn new<State>(
        base_surface: &WlSurface,
        subcompositor: &SubcompositorState,
        queue_handle: &QueueHandle<State>,
    ) -> Self
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        // XXX the order must be in sync with associated constants.
        let parts = [
            // Header.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                0,
                HEADER_SIZE,
                (0, -(HEADER_SIZE as i32)),
            ),
            // Top.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                0,
                BORDER_SIZE,
                (
                    -(BORDER_SIZE as i32),
                    -(HEADER_SIZE as i32 + BORDER_SIZE as i32),
                ),
            ),
            // Left.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                BORDER_SIZE,
                0,
                (-(BORDER_SIZE as i32), -(HEADER_SIZE as i32)),
            ),
            // Right.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                BORDER_SIZE,
                0,
                (-(BORDER_SIZE as i32), -(HEADER_SIZE as i32)),
            ),
            // Bottom.
            Part::new(
                base_surface,
                subcompositor,
                queue_handle,
                0,
                BORDER_SIZE,
                (-(BORDER_SIZE as i32), 0),
            ),
        ];

        Self { parts }
    }

    pub fn parts(&self) -> std::iter::Enumerate<std::slice::Iter<Part>> {
        self.parts.iter().enumerate()
    }

    pub fn hide(&self) {
        for part in self.parts.iter() {
            part.surface.attach(None, 0, 0);
            part.surface.commit();
        }
    }

    pub fn hide_borders(&self) {
        for (_, part) in self.parts().filter(|(idx, _)| *idx != Self::HEADER) {
            part.surface.attach(None, 0, 0);
            part.surface.commit();
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.parts[Self::HEADER].width = width;

        self.parts[Self::BOTTOM].width = width + 2 * BORDER_SIZE;
        self.parts[Self::BOTTOM].pos.1 = height as i32;

        self.parts[Self::TOP].width = self.parts[Self::BOTTOM].width;

        self.parts[Self::LEFT].height = height + HEADER_SIZE;

        self.parts[Self::RIGHT].height = self.parts[Self::LEFT].height;
        self.parts[Self::RIGHT].pos.0 = width as i32;
    }

    pub fn header(&self) -> &Part {
        &self.parts[Self::HEADER]
    }

    pub fn find_surface(&self, surface: &WlSurface) -> Location {
        let pos = match self.parts.iter().position(|part| &part.surface == surface) {
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

#[derive(Debug)]
pub struct Part {
    pub surface: WlSurface,
    pub subsurface: WlSubsurface,

    pub width: u32,
    pub height: u32,

    pub pos: (i32, i32),
}

impl Part {
    fn new<State>(
        parent: &WlSurface,
        subcompositor: &SubcompositorState,
        queue_handle: &QueueHandle<State>,
        width: u32,
        height: u32,
        pos: (i32, i32),
    ) -> Part
    where
        State: Dispatch<WlSurface, SurfaceData> + Dispatch<WlSubsurface, SubsurfaceData> + 'static,
    {
        let (subsurface, surface) = subcompositor.create_subsurface(parent.clone(), queue_handle);

        // Sync with the parent surface.
        subsurface.set_sync();

        Part {
            surface,
            subsurface,
            width,
            height,
            pos,
        }
    }

    pub fn scale(&self) -> u32 {
        self.surface.data::<SurfaceData>().unwrap().scale_factor() as u32
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}
