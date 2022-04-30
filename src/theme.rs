use smithay_client_toolkit::window::WindowState;
use tiny_skia::{Color, Paint, Shader};

pub const BORDER_SIZE: u32 = 10;
pub const HEADER_SIZE: u32 = 35;

// Border CF CF CF

#[derive(Debug)]
pub struct ColorMap {
    pub headerbar: Color,
    pub button_idle: Color,
    pub button_hover: Color,
    pub button_icon: Color,
    pub border_color: Color,
    pub font_color: Color,
}

impl ColorMap {
    pub fn headerbar_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.headerbar),
            anti_alias: true,
            ..Default::default()
        }
    }

    pub fn button_idle_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.button_idle),
            anti_alias: true,
            ..Default::default()
        }
    }

    pub fn button_hover_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.button_hover),
            anti_alias: true,
            ..Default::default()
        }
    }

    pub fn button_icon_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.button_icon),
            ..Default::default()
        }
    }

    pub fn border_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.border_color),
            ..Default::default()
        }
    }

    pub fn font_paint(&self) -> Paint {
        Paint {
            shader: Shader::SolidColor(self.font_color),
            anti_alias: true,
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub struct ColorTheme {
    pub active: ColorMap,
    pub inactive: ColorMap,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            active: ColorMap {
                headerbar: Color::from_rgba8(235, 235, 235, 255),
                button_idle: Color::from_rgba8(216, 216, 216, 255),
                button_hover: Color::from_rgba8(207, 207, 207, 255),
                button_icon: Color::from_rgba8(42, 42, 42, 255),
                border_color: Color::from_rgba8(220, 220, 220, 255),
                font_color: Color::from_rgba8(47, 47, 47, 255),
            },
            inactive: ColorMap {
                headerbar: Color::from_rgba8(250, 250, 250, 255),
                button_idle: Color::from_rgba8(240, 240, 240, 255),
                button_hover: Color::from_rgba8(216, 216, 216, 255),
                button_icon: Color::from_rgba8(148, 148, 148, 255),
                border_color: Color::from_rgba8(220, 220, 220, 255),
                font_color: Color::from_rgba8(150, 150, 150, 255),
            },
        }
    }
}

impl ColorTheme {
    pub fn for_state(&self, state: WindowState) -> &ColorMap {
        if state == WindowState::Active {
            &self.active
        } else {
            &self.inactive
        }
    }
}
