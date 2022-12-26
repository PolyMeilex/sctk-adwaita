use tiny_skia::{
    ClipMask, Color, FillRule, Path, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Point, Rect,
    Stroke, Transform,
};

use crate::{
    buttons::Buttons,
    theme::{ColorMap, HEADER_SIZE},
    title::TitleText,
    Location, SkiaResult,
};

#[derive(Debug, Default)]
pub(crate) struct RenderData {
    pub x: f32,
    pub y: f32,
    pub scale: u32,
    pub window_size: (u32, u32),
    pub surface_size: (u32, u32),
    pub maximized: bool,
    pub tiled: bool,
    pub resizable: bool,
}

pub(crate) fn render(
    pixmap: &mut PixmapMut,
    buttons: &mut Buttons,
    colors: &ColorMap,
    title_text: Option<&mut TitleText>,
    mouses: &[Location],
    data: RenderData,
) {
    buttons.arrange(
        (data.x as u32, data.y as u32),
        data.surface_size.0,
        data.scale,
    );

    pixmap.fill(Color::TRANSPARENT);

    let header_height = HEADER_SIZE * data.scale as u32;
    let header_width = data.window_size.0 * data.scale as u32;

    let scale = data.scale;

    self::draw_decoration_background(
        pixmap,
        scale as f32,
        (data.x, data.y),
        #[allow(clippy::identity_op)]
        (
            data.window_size.0 * scale + 1 * scale,
            data.window_size.1 * scale + header_height,
        ),
        colors,
        data.maximized,
        data.tiled,
    );

    if let Some(text_pixmap) = title_text.and_then(|t| t.pixmap()) {
        self::draw_title(
            pixmap,
            text_pixmap,
            (data.x, data.y),
            (header_width, header_height),
            buttons,
        );
    }

    if buttons.close.x() > data.x {
        buttons.draw_close(scale as f32, colors, mouses, pixmap);
    }

    if buttons.maximize.x() > data.x {
        buttons.draw_maximize(
            scale as f32,
            colors,
            mouses,
            data.resizable,
            data.maximized,
            pixmap,
        );
    }

    if buttons.minimize.x() > data.x {
        buttons.draw_minimize(scale as f32, colors, mouses, pixmap);
    }
}

fn draw_title(
    pixmap: &mut PixmapMut,
    text_pixmap: &Pixmap,
    (margin_h, margin_v): (f32, f32),
    (header_w, header_h): (u32, u32),
    buttons: &Buttons,
) {
    let canvas_w = pixmap.width() as f32;
    let canvas_h = pixmap.height() as f32;

    let header_w = header_w as f32;
    let header_h = header_h as f32;

    let text_w = text_pixmap.width() as f32;
    let text_h = text_pixmap.height() as f32;

    let x = header_w / 2.0 - text_w / 2.0;
    let y = header_h / 2.0 - text_h / 2.0;

    let x = margin_h + x;
    let y = margin_v + y;

    let (x, y) = if x + text_w < buttons.minimize.x() - 10.0 {
        (x, y)
    } else {
        let y = header_h / 2.0 - text_h / 2.0;

        let x = buttons.minimize.x() - text_w - 10.0;
        let y = margin_v + y;
        (x, y)
    };

    let x = x.max(margin_h + 5.0);

    if let Some(clip) = Rect::from_xywh(0.0, 0.0, buttons.minimize.x() - 10.0, canvas_h) {
        let mut mask = ClipMask::new();
        mask.set_path(
            canvas_w as u32,
            canvas_h as u32,
            &PathBuilder::from_rect(clip),
            FillRule::Winding,
            false,
        );

        pixmap.draw_pixmap(
            x as i32,
            y as i32,
            text_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }
}

fn draw_decoration_background(
    pixmap: &mut PixmapMut,
    scale: f32,
    (margin_h, margin_v): (f32, f32),
    (width, height): (u32, u32),
    colors: &ColorMap,
    is_maximized: bool,
    tiled: bool,
) -> SkiaResult {
    let radius = if is_maximized || tiled {
        0.0
    } else {
        10.0 * scale
    };

    let width = width as f32;
    let height = height as f32;

    let margin_h = margin_h - 1.0 * scale;

    let bg = rounded_headerbar_shape(margin_h, margin_v, width, height, radius)?;
    let header = rounded_headerbar_shape(
        margin_h,
        margin_v,
        width,
        HEADER_SIZE as f32 * scale,
        radius,
    )?;

    pixmap.fill_path(
        &header,
        &colors.headerbar_paint(),
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    pixmap.stroke_path(
        &bg,
        &colors.border_paint(),
        &Stroke {
            width: 1.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );

    Some(())
}

fn rounded_headerbar_shape(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    use std::f32::consts::FRAC_1_SQRT_2;

    let mut pb = PathBuilder::new();
    let mut cursor = Point::from_xy(x, y);

    // !!!
    // This code is heavily "inspired" by https://gitlab.com/snakedye/snui/
    // So technically it should be licensed under MPL-2.0, sorry about that 🥺 👉👈
    // !!!

    // Positioning the cursor
    cursor.y += radius;
    pb.move_to(cursor.x, cursor.y);

    // Drawing the outline
    pb.cubic_to(
        cursor.x,
        cursor.y,
        cursor.x,
        cursor.y - FRAC_1_SQRT_2 * radius,
        {
            cursor.x += radius;
            cursor.x
        },
        {
            cursor.y -= radius;
            cursor.y
        },
    );
    pb.line_to(
        {
            cursor.x = x + width - radius;
            cursor.x
        },
        cursor.y,
    );
    pb.cubic_to(
        cursor.x,
        cursor.y,
        cursor.x + FRAC_1_SQRT_2 * radius,
        cursor.y,
        {
            cursor.x += radius;
            cursor.x
        },
        {
            cursor.y += radius;
            cursor.y
        },
    );
    pb.line_to(cursor.x, {
        cursor.y = y + height;
        cursor.y
    });
    pb.line_to(
        {
            cursor.x = x;
            cursor.x
        },
        cursor.y,
    );

    pb.close();

    pb.finish()
}
