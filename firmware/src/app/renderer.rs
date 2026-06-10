use std::mem::{discriminant, take};
use std::time::{Duration, Instant};

use crate::app::ui::{
    color, logical_rect_to_physical_area, Color, FontFace, Rect, TextAlign, UiCanvas, UI_WIDTH,
};
use crate::drivers::display::{EspResult, Sh8601};

#[derive(Clone, PartialEq)]
pub struct Scene {
    objects: Vec<UiObject>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    pub fn push(&mut self, object: UiObject) {
        self.objects.push(object);
    }

    fn draw_dirty(&self, ui: &mut UiCanvas<'_>, frame: u32, dirty: Rect) {
        for object in &self.objects {
            if object.bounds(frame).intersects(dirty) {
                object.draw(ui, frame);
            }
        }
    }

    fn animation_interval(&self) -> Option<Duration> {
        self.objects
            .iter()
            .filter_map(UiObject::animation_interval)
            .min()
    }

    fn animated_bounds(&self, frame: u32) -> Option<Rect> {
        self.objects
            .iter()
            .filter(|object| object.is_animated())
            .map(|object| object.bounds(frame))
            .reduce(Rect::union)
    }

    fn diff_rects(&self, next: &Self, frame: u32) -> Vec<Rect> {
        if !self.has_same_shape(next) {
            return vec![Rect::full()];
        }

        self.objects
            .iter()
            .zip(next.objects.iter())
            .filter(|(current, next)| current != next)
            .map(|(current, next)| current.bounds(frame).union(next.bounds(0)))
            .filter(|rect| !rect.is_empty())
            .collect()
    }

    fn coalesced_diff_rects(&self, next: &Self, frame: u32) -> Vec<Rect> {
        let rects = self.diff_rects(next, frame);
        let Some(rect) = rects.into_iter().reduce(Rect::union) else {
            return Vec::new();
        };
        vec![rect]
    }

    fn has_same_shape(&self, next: &Self) -> bool {
        self.objects.len() == next.objects.len()
            && self
                .objects
                .iter()
                .zip(next.objects.iter())
                .all(|(current, next)| current.kind() == next.kind())
    }
}

#[derive(Clone, PartialEq)]
pub enum UiObject {
    RoundedRect(RoundedRectObject),
    Text(TextObject),
    PixelArt(PixelArtObject),
    RoundedMeterFill(RoundedMeterFillObject),
    LoadingDots(LoadingDotsObject),
}

impl UiObject {
    pub fn rounded_rect(x: i32, y: i32, w: i32, h: i32, radius: i32, color: Color) -> Self {
        Self::RoundedRect(RoundedRectObject {
            bounds: Rect::new(x, y, w, h),
            radius,
            color,
        })
    }

    pub fn text(
        x: i32,
        y: i32,
        width: i32,
        text: impl Into<String>,
        scale: i32,
        color: Color,
        align: TextAlign,
    ) -> Self {
        Self::text_with_font(x, y, width, text, FontFace::DEFAULT, scale, color, align)
    }

    pub fn text_with_font(
        x: i32,
        y: i32,
        width: i32,
        text: impl Into<String>,
        font: FontFace,
        scale: i32,
        color: Color,
        align: TextAlign,
    ) -> Self {
        Self::Text(TextObject {
            x,
            y,
            width,
            text: text.into(),
            font,
            scale,
            color,
            align,
        })
    }

    pub fn rounded_meter_fill(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        percent: u8,
        radius: i32,
        fill_color: Color,
        track_color: Color,
    ) -> Self {
        Self::RoundedMeterFill(RoundedMeterFillObject {
            bounds: Rect::new(x, y, w, h),
            percent,
            radius,
            fill_color,
            track_color,
        })
    }

    pub fn pixel_art(
        x: i32,
        y: i32,
        pixel: i32,
        width: i32,
        height: i32,
        cells: Vec<u8>,
        palette: Vec<Color>,
    ) -> Self {
        Self::PixelArt(PixelArtObject {
            bounds: Rect::new(x, y, width * pixel, height * pixel),
            pixel,
            width,
            cells,
            palette,
        })
    }

    pub fn loading_dots(base_x: i32, base_y: i32) -> Self {
        Self::LoadingDots(LoadingDotsObject {
            base_x,
            base_y,
            color: color::TEAL,
            frame_ms: 33,
        })
    }

    fn draw(&self, ui: &mut UiCanvas<'_>, frame: u32) {
        match self {
            Self::RoundedRect(object) => ui.rounded_rect(
                object.bounds.x,
                object.bounds.y,
                object.bounds.w,
                object.bounds.h,
                object.radius,
                object.color,
            ),
            Self::Text(object) => ui.text_with_font(
                object.x,
                object.y,
                object.width,
                object.text.as_str(),
                object.font,
                object.scale,
                object.color,
                object.align,
            ),
            Self::PixelArt(object) => object.draw(ui),
            Self::RoundedMeterFill(object) => ui.rounded_meter_fill(
                object.bounds.x,
                object.bounds.y,
                object.bounds.w,
                object.bounds.h,
                object.percent,
                object.radius,
                object.fill_color,
                object.track_color,
            ),
            Self::LoadingDots(object) => object.draw(ui, frame),
        }
    }

    fn bounds(&self, frame: u32) -> Rect {
        match self {
            Self::RoundedRect(object) => object.bounds,
            Self::Text(object) => object.bounds(),
            Self::PixelArt(object) => object.bounds,
            Self::RoundedMeterFill(object) => object.bounds,
            Self::LoadingDots(object) => object.bounds(frame),
        }
        .clamp_to_screen()
    }

    fn animation_interval(&self) -> Option<Duration> {
        match self {
            Self::LoadingDots(object) => Some(Duration::from_millis(object.frame_ms)),
            _ => None,
        }
    }

    fn is_animated(&self) -> bool {
        self.animation_interval().is_some()
    }

    fn kind(&self) -> UiObjectKind {
        UiObjectKind(discriminant(self))
    }
}

#[derive(Eq, PartialEq)]
struct UiObjectKind(std::mem::Discriminant<UiObject>);

#[derive(Clone, PartialEq)]
pub struct RoundedRectObject {
    bounds: Rect,
    radius: i32,
    color: Color,
}

#[derive(Clone, PartialEq)]
pub struct TextObject {
    x: i32,
    y: i32,
    width: i32,
    text: String,
    font: FontFace,
    scale: i32,
    color: Color,
    align: TextAlign,
}

impl TextObject {
    fn bounds(&self) -> Rect {
        let lines = self.text.lines().count();
        Rect::new(
            self.x,
            self.y,
            self.width,
            UiCanvas::text_height_for(self.font, self.scale, lines),
        )
        .expand(2)
    }
}

#[derive(Clone, PartialEq)]
pub struct RoundedMeterFillObject {
    bounds: Rect,
    percent: u8,
    radius: i32,
    fill_color: Color,
    track_color: Color,
}

#[derive(Clone, PartialEq)]
pub struct PixelArtObject {
    bounds: Rect,
    pixel: i32,
    width: i32,
    cells: Vec<u8>,
    palette: Vec<Color>,
}

impl PixelArtObject {
    fn draw(&self, ui: &mut UiCanvas<'_>) {
        let width = self.width.max(0) as usize;
        if width == 0 {
            return;
        }
        for (row_index, row) in self.cells.chunks(width).enumerate() {
            for (column_index, cell) in row.iter().enumerate() {
                if *cell == 0 {
                    continue;
                }
                if let Some(color) = self.palette.get((*cell - 1) as usize) {
                    ui.rect(
                        self.bounds.x + column_index as i32 * self.pixel,
                        self.bounds.y + row_index as i32 * self.pixel,
                        self.pixel,
                        self.pixel,
                        *color,
                    );
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct LoadingDotsObject {
    base_x: i32,
    base_y: i32,
    color: Color,
    frame_ms: u64,
}

impl LoadingDotsObject {
    fn draw(&self, ui: &mut UiCanvas<'_>, frame: u32) {
        for index in 0..3 {
            let (x, y) = self.dot_position(frame, index);
            ui.circle(x, y, 8, self.color);
        }
    }

    fn bounds(&self, _frame: u32) -> Rect {
        Rect::new(0, self.base_y - 25, UI_WIDTH as i32, 50).clamp_to_screen()
    }

    fn dot_position(&self, frame: u32, index: i32) -> (i32, i32) {
        let wave_offsets = [0, -5, -9, -12, -9, -5, 0, 5, 9, 12, 9, 5];
        let phase = (frame as usize + index as usize * 3) % wave_offsets.len();
        (self.base_x + index * 34, self.base_y + wave_offsets[phase])
    }
}

pub struct Renderer {
    scene: Option<Scene>,
    rendered_scene: Option<Scene>,
    dirty: Vec<Rect>,
    frame: u32,
    last_frame: Instant,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            scene: None,
            rendered_scene: None,
            dirty: Vec::new(),
            frame: 0,
            last_frame: Instant::now(),
        }
    }

    pub fn set_scene(&mut self, scene: Scene) {
        if self.scene.as_ref() == Some(&scene) {
            return;
        }

        self.dirty = match &self.rendered_scene {
            Some(rendered_scene) => rendered_scene.coalesced_diff_rects(&scene, self.frame),
            None => vec![Rect::full()],
        };
        self.scene = Some(scene);
        self.frame = 0;
        self.last_frame = Instant::now();
    }

    pub fn tick(&mut self, panel: &Sh8601) -> EspResult {
        let animated_dirty = {
            let Some(scene) = self.scene.as_ref() else {
                return Ok(());
            };

            if self.rendered_scene.as_ref() == Some(scene) {
                scene.animation_interval().and_then(|interval| {
                    if self.last_frame.elapsed() >= interval {
                        let previous = scene.animated_bounds(self.frame);
                        self.frame = self.frame.wrapping_add(1);
                        let current = scene.animated_bounds(self.frame);
                        self.last_frame = Instant::now();
                        previous.into_iter().chain(current).reduce(Rect::union)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        };
        if let Some(bounds) = animated_dirty {
            self.mark_dirty(bounds);
        }

        if self.dirty.is_empty() {
            return Ok(());
        }

        let dirty_rects = take(&mut self.dirty);
        let Some(scene) = self.scene.as_ref() else {
            return Ok(());
        };
        for dirty in dirty_rects {
            self.draw(panel, scene, dirty)?;
        }
        self.rendered_scene.clone_from(&self.scene);
        Ok(())
    }

    fn mark_dirty(&mut self, rect: Rect) {
        let rect = rect.clamp_to_screen();
        if rect.is_empty() {
            return;
        }

        self.dirty.push(rect);
    }

    fn draw(&self, panel: &Sh8601, scene: &Scene, dirty: Rect) -> EspResult {
        let Some(area) = logical_rect_to_physical_area(dirty) else {
            return Ok(());
        };

        panel.draw_area(
            area.x,
            area.y,
            area.w,
            area.h,
            |output, x, y, width, rows| {
                let mut ui = UiCanvas::new_area(output, x, y, width, rows);
                ui.dotted_background();
                scene.draw_dirty(&mut ui, self.frame, dirty);
            },
        )
    }
}
