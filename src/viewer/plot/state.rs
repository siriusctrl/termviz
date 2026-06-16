use crate::plot::model::PlotBounds;

const PAN_STEP: f64 = 0.15;
const ZOOM_STEP: f64 = 1.2;
const MIN_SPAN: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlotViewState {
    pub(super) full: PlotBounds,
    pub(super) visible: PlotBounds,
    pub(super) fit_mode: bool,
}

impl PlotViewState {
    pub(super) fn new(full: PlotBounds) -> Self {
        Self {
            full,
            visible: full,
            fit_mode: true,
        }
    }

    pub(super) fn pan_left(&mut self) {
        self.pan_horizontal(-1.0);
    }

    pub(super) fn pan_right(&mut self) {
        self.pan_horizontal(1.0);
    }

    pub(super) fn pan_up(&mut self) {
        self.pan_vertical(1.0);
    }

    pub(super) fn pan_down(&mut self) {
        self.pan_vertical(-1.0);
    }

    fn pan_horizontal(&mut self, direction: f64) {
        let span = (self.visible.x_max - self.visible.x_min)
            .abs()
            .max(MIN_SPAN);
        let step = span * PAN_STEP;
        let next_start = self.visible.x_min + step * direction;
        let previous = self.visible;
        self.visible.x_min = next_start;
        self.visible.x_max = next_start + span;
        self.clamp_visible_x();
        self.fit_mode = spans_are_full(self.visible, self.full);
        if self.visible != previous {
            self.fit_mode = false;
        }
    }

    fn pan_vertical(&mut self, direction: f64) {
        let span = (self.visible.y_max - self.visible.y_min)
            .abs()
            .max(MIN_SPAN);
        let step = span * PAN_STEP;
        let next_start = self.visible.y_min + step * direction;
        let previous = self.visible;
        self.visible.y_min = next_start;
        self.visible.y_max = next_start + span;
        self.clamp_visible_y();
        self.fit_mode = spans_are_full(self.visible, self.full);
        if self.visible != previous {
            self.fit_mode = false;
        }
    }

    pub(super) fn zoom_in(&mut self) {
        self.zoom(ZOOM_STEP);
    }

    pub(super) fn zoom_out(&mut self) {
        self.zoom(1.0 / ZOOM_STEP);
    }

    pub(super) fn reset(&mut self) {
        self.visible = self.full;
        self.fit_mode = true;
    }

    fn zoom(&mut self, factor: f64) {
        if factor <= 0.0 {
            return;
        }

        let full_x = (self.full.x_max - self.full.x_min).abs().max(MIN_SPAN);
        let full_y = (self.full.y_max - self.full.y_min).abs().max(MIN_SPAN);
        let current_x = (self.visible.x_max - self.visible.x_min)
            .abs()
            .max(MIN_SPAN);
        let current_y = (self.visible.y_max - self.visible.y_min)
            .abs()
            .max(MIN_SPAN);

        let next_x = (current_x / factor).clamp(MIN_SPAN, full_x);
        let next_y = (current_y / factor).clamp(MIN_SPAN, full_y);

        let x_center = (self.visible.x_min + self.visible.x_max) / 2.0;
        let y_center = (self.visible.y_min + self.visible.y_max) / 2.0;

        self.visible.x_min = x_center - next_x / 2.0;
        self.visible.x_max = x_center + next_x / 2.0;
        self.visible.y_min = y_center - next_y / 2.0;
        self.visible.y_max = y_center + next_y / 2.0;
        self.clamp_visible_x();
        self.clamp_visible_y();
        self.fit_mode = spans_are_full(self.visible, self.full);
    }

    fn clamp_visible_x(&mut self) {
        self.visible.x_min = min_value(self.visible.x_min, self.visible.x_max).0;
        self.visible.x_max = min_value(self.visible.x_min, self.visible.x_max).1;
        let (next_min, next_max) = clamp_axis_window(
            self.visible.x_min,
            self.visible.x_max,
            self.full.x_min,
            self.full.x_max,
        );
        self.visible.x_min = next_min;
        self.visible.x_max = next_max;
    }

    fn clamp_visible_y(&mut self) {
        self.visible.y_min = min_value(self.visible.y_min, self.visible.y_max).0;
        self.visible.y_max = min_value(self.visible.y_min, self.visible.y_max).1;
        let (next_min, next_max) = clamp_axis_window(
            self.visible.y_min,
            self.visible.y_max,
            self.full.y_min,
            self.full.y_max,
        );
        self.visible.y_min = next_min;
        self.visible.y_max = next_max;
    }
}

fn min_value(first: f64, second: f64) -> (f64, f64) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

pub(super) fn spans_are_full(visible: PlotBounds, full: PlotBounds) -> bool {
    let full_x = (full.x_max - full.x_min).abs().max(MIN_SPAN);
    let full_y = (full.y_max - full.y_min).abs().max(MIN_SPAN);
    let visible_x = (visible.x_max - visible.x_min).abs().abs().max(MIN_SPAN);
    let visible_y = (visible.y_max - visible.y_min).abs().abs().max(MIN_SPAN);
    (visible_x - full_x).abs() < 1e-9 && (visible_y - full_y).abs() < 1e-9
}

pub(super) fn clamp_axis_window(start: f64, end: f64, full_min: f64, full_max: f64) -> (f64, f64) {
    let (start, end) = min_value(start, end);
    let (full_min, full_max) = min_value(full_min, full_max);
    let span = (end - start).abs().max(MIN_SPAN);
    let full_span = (full_max - full_min).abs().max(MIN_SPAN);

    if span >= full_span {
        return (full_min, full_max);
    }

    let mut next_start = start.clamp(full_min, full_max);
    let mut next_end = next_start + span;
    if next_end > full_max {
        next_end = full_max;
        next_start = next_end - span;
    }

    (next_start, next_end)
}
