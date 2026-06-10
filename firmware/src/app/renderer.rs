use std::mem::{discriminant, take};
use std::time::{Duration, Instant};

use crate::app::ui::{
    color, logical_rect_to_physical_area, Color, Rect, TextAlign, UiCanvas, UI_WIDTH,
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
    Rect(RectObject),
    Text(TextObject),
    Meter(MeterObject),
    MeterFill(MeterFillObject),
    LoadingDots(LoadingDotsObject),
}

impl UiObject {
    pub fn rect(x: i32, y: i32, w: i32, h: i32, color: Color) -> Self {
        Self::Rect(RectObject {
            bounds: Rect::new(x, y, w, h),
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
        Self::Text(TextObject {
            x,
            y,
            width,
            text: text.into(),
            scale,
            color,
            align,
        })
    }

    pub fn meter(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        percent: u8,
        fill_color: Color,
        border_color: Color,
    ) -> Self {
        Self::Meter(MeterObject {
            bounds: Rect::new(x, y, w, h),
            percent,
            fill_color,
            border_color,
        })
    }

    pub fn meter_fill(x: i32, y: i32, w: i32, h: i32, percent: u8, fill_color: Color) -> Self {
        Self::MeterFill(MeterFillObject {
            bounds: Rect::new(x, y, w, h),
            percent,
            fill_color,
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
            Self::Rect(object) => ui.rect(
                object.bounds.x,
                object.bounds.y,
                object.bounds.w,
                object.bounds.h,
                object.color,
            ),
            Self::Text(object) => ui.text(
                object.x,
                object.y,
                object.width,
                object.text.as_str(),
                object.scale,
                object.color,
                object.align,
            ),
            Self::Meter(object) => {
                ui.meter_shell(
                    object.bounds.x,
                    object.bounds.y,
                    object.bounds.w,
                    object.bounds.h,
                    object.border_color,
                );
                ui.meter_fill(
                    object.bounds.x + 4,
                    object.bounds.y + 4,
                    object.bounds.w - 8,
                    object.bounds.h - 8,
                    object.percent,
                    object.fill_color,
                );
            }
            Self::MeterFill(object) => ui.meter_fill(
                object.bounds.x,
                object.bounds.y,
                object.bounds.w,
                object.bounds.h,
                object.percent,
                object.fill_color,
            ),
            Self::LoadingDots(object) => object.draw(ui, frame),
        }
    }

    fn bounds(&self, frame: u32) -> Rect {
        match self {
            Self::Rect(object) => object.bounds,
            Self::Text(object) => object.bounds(),
            Self::Meter(object) => object.bounds,
            Self::MeterFill(object) => object.bounds,
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
pub struct RectObject {
    bounds: Rect,
    color: Color,
}

#[derive(Clone, PartialEq)]
pub struct TextObject {
    x: i32,
    y: i32,
    width: i32,
    text: String,
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
            UiCanvas::text_height(self.scale, lines),
        )
        .expand(2)
    }
}

#[derive(Clone, PartialEq)]
pub struct MeterObject {
    bounds: Rect,
    percent: u8,
    fill_color: Color,
    border_color: Color,
}

#[derive(Clone, PartialEq)]
pub struct MeterFillObject {
    bounds: Rect,
    percent: u8,
    fill_color: Color,
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
        let Some(scene) = self.scene.clone() else {
            return Ok(());
        };

        if self.rendered_scene.as_ref() == Some(&scene) {
            if let Some(interval) = scene.animation_interval() {
                if self.last_frame.elapsed() >= interval {
                    let previous = scene.animated_bounds(self.frame);
                    self.frame = self.frame.wrapping_add(1);
                    let current = scene.animated_bounds(self.frame);
                    self.last_frame = Instant::now();
                    if let Some(bounds) = previous.into_iter().chain(current).reduce(Rect::union) {
                        self.mark_dirty(bounds);
                    }
                }
            }
        }

        if self.dirty.is_empty() {
            return Ok(());
        }

        for dirty in take(&mut self.dirty) {
            self.draw(panel, &scene, dirty)?;
        }
        self.rendered_scene = Some(scene);
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
