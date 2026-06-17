use std::{
    borrow::Cow,
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use image::RgbaImage;

use crate::{
    plot::{
        PlotKind,
        model::{PlotBounds, PlotScene},
    },
    render::{
        Protocol,
        protocols::{ProtocolRenderContext, blocks, kitty, render_plot_rgba_with_fallback},
    },
    tui::TerminalSize,
};

use super::{
    atlas::render_pan_prefetch_frames,
    chrome::{pixel_protocol_target_size, plot_protocol_layout},
    state::{PlotNavAction, PlotViewState},
};

const MAX_CACHED_FRAMES: usize = 48;
const MAX_IN_FLIGHT_PREFETCHES: usize = 3;
const PREFETCH_FORWARD_STEPS: usize = 3;
const PREFETCH_GRACE_PERIOD: Duration = Duration::from_millis(12);
const PAN_ATLAS_MIN_POINTS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlotFrameCacheKey {
    pub(super) kind: PlotKind,
    pub(super) protocol: Protocol,
    pub(super) visible: PlotBounds,
    pub(super) size: TerminalSize,
}

pub(super) struct PlotFrameCache {
    entries: VecDeque<CachedPlotFrame>,
    pub(super) last: Option<CachedPlotFrame>,
    prefetch_tx: Sender<PrefetchResult>,
    prefetch_rx: Receiver<PrefetchResult>,
    in_flight: Arc<AtomicUsize>,
    queued: Vec<PlotFrameCacheKey>,
    next_image_id: u32,
    next_transmit_batch: u64,
    min_transmit_priority: u64,
    visible_image_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub(super) struct CachedPlotFrame {
    pub(super) key: PlotFrameCacheKey,
    display_payload: String,
    transmit_payload: Option<String>,
    place_payload: Option<String>,
    image_id: Option<u32>,
    transmitted: bool,
    transmit_priority: u64,
}

impl CachedPlotFrame {
    fn display_only(key: PlotFrameCacheKey, display_payload: String) -> Self {
        Self {
            key,
            display_payload,
            transmit_payload: None,
            place_payload: None,
            image_id: None,
            transmitted: false,
            transmit_priority: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PrefetchRequest {
    pub(super) key: PlotFrameCacheKey,
    pub(super) image_id: u32,
    pub(super) transmit_priority: u64,
}

#[derive(Debug)]
struct PrefetchResult {
    key: PlotFrameCacheKey,
    frame: Result<CachedPlotFrame, String>,
}

impl Default for PlotFrameCache {
    fn default() -> Self {
        let (prefetch_tx, prefetch_rx) = mpsc::channel();
        Self {
            entries: VecDeque::new(),
            last: None,
            prefetch_tx,
            prefetch_rx,
            in_flight: Arc::new(AtomicUsize::new(0)),
            queued: Vec::new(),
            next_image_id: 1,
            next_transmit_batch: 0,
            min_transmit_priority: 0,
            visible_image_id: None,
        }
    }
}

impl PlotFrameCache {
    pub(super) fn get_or_render(
        &mut self,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<Cow<'_, str>> {
        let key = PlotFrameCacheKey {
            kind,
            protocol,
            visible: state.visible,
            size,
        };
        self.collect_prefetches();
        if self.last.as_ref().is_some_and(|cached| cached.key == key) {
            return Ok(self.visible_payload_from_last());
        }
        if let Some(index) = self.entries.iter().position(|cached| cached.key == key) {
            let cached = self.entries.remove(index).expect("cache index exists");
            self.last = Some(cached.clone());
            self.entries.push_back(cached);
            return Ok(self.visible_payload_from_last());
        }

        let frame = self.render_plot_frame(scene, kind, state, protocol, size)?;
        self.insert_visible(frame);
        Ok(self.visible_payload_from_last())
    }

    pub(super) fn prefetch_neighbors(
        &mut self,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
        recent_action: Option<PlotNavAction>,
    ) {
        self.collect_prefetches();
        if protocol == Protocol::Blocks {
            return;
        }
        if self.in_flight.load(Ordering::Relaxed) >= MAX_IN_FLIGHT_PREFETCHES {
            return;
        }

        let Some(action) = recent_action else {
            return;
        };

        let mut candidates = Vec::new();
        let mut biased = *state;
        for _ in 0..PREFETCH_FORWARD_STEPS {
            biased.apply_nav_action(action);
            candidates.push(biased);
        }
        let mut opposite = *state;
        opposite.apply_nav_action(opposite_action(action));
        candidates.push(opposite);

        let mut keys = Vec::new();
        for next_state in candidates {
            if next_state.visible == state.visible {
                continue;
            }
            let key = PlotFrameCacheKey {
                kind,
                protocol,
                visible: next_state.visible,
                size,
            };
            if self.contains_key(key) || self.queued.contains(&key) {
                continue;
            }
            keys.push(key);
        }
        if keys.is_empty() {
            return;
        }
        if matches!(
            action,
            PlotNavAction::PanLeft
                | PlotNavAction::PanRight
                | PlotNavAction::PanUp
                | PlotNavAction::PanDown
        ) {
            let keys = self.keys_with_image_ids(keys);
            if scene.total_points() >= PAN_ATLAS_MIN_POINTS {
                self.spawn_pan_prefetch(scene.clone(), keys);
            } else {
                self.spawn_batch_prefetch(scene.clone(), keys);
            }
        } else {
            let keys = self.keys_with_image_ids(keys);
            self.spawn_batch_prefetch(scene.clone(), keys);
        }
    }

    fn contains_key(&self, key: PlotFrameCacheKey) -> bool {
        self.last.as_ref().is_some_and(|cached| cached.key == key)
            || self.entries.iter().any(|cached| cached.key == key)
    }

    pub(super) fn drain_transmit_payloads(&mut self, max_count: usize) -> Vec<String> {
        self.collect_prefetches();
        let mut payloads = Vec::new();
        for _ in 0..max_count {
            let Some(index) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, cached)| {
                    !cached.transmitted
                        && cached.transmit_payload.is_some()
                        && cached.image_id != self.visible_image_id
                })
                .max_by_key(|(_, cached)| cached.transmit_priority)
                .map(|(index, _)| index)
            else {
                break;
            };
            let cached = self
                .entries
                .get_mut(index)
                .expect("selected transmit cache entry exists");
            let key = cached.key;
            cached.transmitted = true;
            if let Some(last) = self.last.as_mut()
                && last.key == key
            {
                last.transmitted = true;
            }
            if let Some(payload) = cached.transmit_payload.as_ref() {
                payloads.push(payload.clone());
            }
        }
        payloads
    }

    fn insert_visible(&mut self, frame: CachedPlotFrame) {
        let key = frame.key;
        if let Some(index) = self.entries.iter().position(|cached| cached.key == key) {
            self.entries.remove(index);
        }
        self.entries.push_back(frame.clone());
        while self.entries.len() > MAX_CACHED_FRAMES {
            self.entries.pop_front();
        }
        self.last = Some(frame);
    }

    fn insert_prefetched(&mut self, frame: CachedPlotFrame) {
        let key = frame.key;
        if self.last.as_ref().is_some_and(|cached| cached.key == key) {
            return;
        }
        if let Some(index) = self.entries.iter().position(|cached| cached.key == key) {
            self.entries.remove(index);
        }
        self.entries.push_back(frame);
        while self.entries.len() > MAX_CACHED_FRAMES {
            self.entries.pop_front();
        }
    }

    fn visible_payload_from_last(&mut self) -> Cow<'_, str> {
        let frame = self.last.as_mut().expect("last cache entry exists");
        visible_payload_for_frame(&mut self.visible_image_id, frame)
    }

    fn collect_prefetches(&mut self) {
        while let Ok(result) = self.prefetch_rx.try_recv() {
            self.queued.retain(|key| *key != result.key);
            if let Ok(mut frame) = result.frame {
                discard_stale_transmit_payload(&mut frame, self.min_transmit_priority);
                self.insert_prefetched(frame);
            }
        }
    }

    fn keys_with_image_ids(&mut self, keys: Vec<PlotFrameCacheKey>) -> Vec<PrefetchRequest> {
        self.next_transmit_batch = self.next_transmit_batch.wrapping_add(1);
        let batch_base = self.next_transmit_batch.saturating_mul(1_000);
        let len = keys.len() as u64;
        keys.into_iter()
            .enumerate()
            .map(|(index, key)| PrefetchRequest {
                key,
                image_id: self.allocate_image_id(),
                transmit_priority: batch_base + len.saturating_sub(index as u64),
            })
            .collect()
    }

    fn allocate_image_id(&mut self) -> u32 {
        let image_id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        image_id
    }

    fn spawn_batch_prefetch(&mut self, scene: PlotScene, keys: Vec<PrefetchRequest>) {
        self.discard_stale_transmit_payloads(&keys);
        self.queued.extend(keys.iter().map(|request| request.key));
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let tx = self.prefetch_tx.clone();
        let in_flight = Arc::clone(&self.in_flight);
        thread::spawn(move || {
            thread::sleep(PREFETCH_GRACE_PERIOD);
            for request in keys {
                let frame = render_plot_frame_for_key(
                    &scene,
                    request.key,
                    Some(request.image_id),
                    request.transmit_priority,
                )
                .map_err(|error| error.to_string());
                let key = request.key;
                let _ = tx.send(PrefetchResult { key, frame });
            }
            in_flight.fetch_sub(1, Ordering::Relaxed);
        });
    }

    fn spawn_pan_prefetch(&mut self, scene: PlotScene, keys: Vec<PrefetchRequest>) {
        self.discard_stale_transmit_payloads(&keys);
        self.queued.extend(keys.iter().map(|request| request.key));
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let tx = self.prefetch_tx.clone();
        let in_flight = Arc::clone(&self.in_flight);
        thread::spawn(move || {
            thread::sleep(PREFETCH_GRACE_PERIOD);
            for (key, frame) in render_pan_prefetch_frames(&scene, &keys) {
                let _ = tx.send(PrefetchResult { key, frame });
            }
            in_flight.fetch_sub(1, Ordering::Relaxed);
        });
    }

    fn render_plot_frame(
        &mut self,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<CachedPlotFrame> {
        render_plot_frame_for_key(
            scene,
            PlotFrameCacheKey {
                kind,
                protocol,
                visible: state.visible,
                size,
            },
            (protocol == Protocol::Kitty).then(|| self.allocate_image_id()),
            0,
        )
    }

    fn discard_stale_transmit_payloads(&mut self, requests: &[PrefetchRequest]) {
        let Some(min_priority) = requests
            .iter()
            .map(|request| request.transmit_priority)
            .min()
        else {
            return;
        };
        self.min_transmit_priority = self.min_transmit_priority.max(min_priority);
        for cached in &mut self.entries {
            discard_stale_transmit_payload(cached, self.min_transmit_priority);
        }
        if let Some(last) = self.last.as_mut() {
            discard_stale_transmit_payload(last, self.min_transmit_priority);
        }
    }
}

fn discard_stale_transmit_payload(frame: &mut CachedPlotFrame, min_priority: u64) {
    if !frame.transmitted && frame.transmit_priority < min_priority {
        frame.transmit_payload = None;
        frame.place_payload = None;
    }
}

pub(super) fn render_plot_frame(
    scene: &PlotScene,
    kind: PlotKind,
    state: &PlotViewState,
    protocol: Protocol,
    size: TerminalSize,
) -> Result<String> {
    Ok(render_plot_frame_for_key(
        scene,
        PlotFrameCacheKey {
            kind,
            protocol,
            visible: state.visible,
            size,
        },
        None,
        0,
    )?
    .display_payload)
}

pub(super) fn render_plot_frame_for_key(
    scene: &PlotScene,
    key: PlotFrameCacheKey,
    image_id: Option<u32>,
    transmit_priority: u64,
) -> Result<CachedPlotFrame> {
    let cols = u32::from(key.size.width);
    let rows = u32::from(key.size.height.saturating_sub(1)).max(1);
    if cols == 0 || rows == 0 {
        return Ok(CachedPlotFrame::display_only(key, String::new()));
    }

    if key.protocol == Protocol::Blocks {
        let drawable_cols = u32::from(key.size.width.saturating_sub(1).max(1));
        let payload = blocks::render_terminal_plot_for_size(
            scene,
            key.kind,
            key.visible,
            drawable_cols,
            rows,
        )
        .context("rendering terminal plot frame");
        return Ok(CachedPlotFrame::display_only(key, payload?));
    }

    let layout = plot_protocol_layout(key.size);
    let target = pixel_protocol_target_size(
        key.protocol,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    );
    let image = crate::render::protocols::plot::render_interactive_plot_body_rgba_for_size(
        scene,
        key.kind,
        key.visible,
        target.0,
        target.1,
    )?;
    prepared_plot_frame_from_rgba(key, &image, image_id, transmit_priority)
}

pub(super) fn prepared_plot_frame_from_rgba(
    key: PlotFrameCacheKey,
    image: &RgbaImage,
    image_id: Option<u32>,
    transmit_priority: u64,
) -> Result<CachedPlotFrame> {
    let layout = plot_protocol_layout(key.size);
    let context = ProtocolRenderContext::new(key.protocol);
    let Some(image_id) = image_id.filter(|_| key.protocol == Protocol::Kitty) else {
        let display_payload = render_plot_rgba_with_fallback(
            context,
            image,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        );
        return Ok(CachedPlotFrame::display_only(key, display_payload));
    };
    let display_payload = kitty::render_rgba_zlib_for_size_with_id(
        image,
        image_id,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    )?;
    let transmit_payload = kitty::transmit_rgba_zlib_with_id(image, image_id)?;
    let place_payload = kitty::place_image_for_size(
        image_id,
        u32::from(layout.image_cols),
        u32::from(layout.image_rows),
    );
    Ok(CachedPlotFrame {
        key,
        display_payload,
        transmit_payload: Some(transmit_payload),
        place_payload: Some(place_payload),
        image_id: Some(image_id),
        transmitted: false,
        transmit_priority,
    })
}

fn cached_frame(cached: &CachedPlotFrame) -> &str {
    if cached.transmitted
        && let Some(place_payload) = cached.place_payload.as_ref()
    {
        return place_payload;
    }
    cached.display_payload.as_str()
}

fn visible_payload_for_frame<'a>(
    visible_image_id: &mut Option<u32>,
    cached: &'a mut CachedPlotFrame,
) -> Cow<'a, str> {
    let Some(current_image_id) = cached.image_id else {
        *visible_image_id = None;
        return Cow::Borrowed(cached_frame(cached));
    };

    let payload = if cached.transmitted {
        Cow::Borrowed(cached_frame(cached))
    } else {
        cached.transmitted = true;
        Cow::Owned(cached.display_payload.clone())
    };
    let previous_image_id = visible_image_id.replace(current_image_id);
    if let Some(previous_image_id) = previous_image_id
        && previous_image_id != current_image_id
    {
        let cleanup = kitty::delete_image_placement(previous_image_id, 1);
        return Cow::Owned(format!("{}{cleanup}", payload.as_ref()));
    }
    payload
}

fn opposite_action(action: PlotNavAction) -> PlotNavAction {
    match action {
        PlotNavAction::PanLeft => PlotNavAction::PanRight,
        PlotNavAction::PanRight => PlotNavAction::PanLeft,
        PlotNavAction::PanUp => PlotNavAction::PanDown,
        PlotNavAction::PanDown => PlotNavAction::PanUp,
        PlotNavAction::ZoomIn => PlotNavAction::ZoomOut,
        PlotNavAction::ZoomOut => PlotNavAction::ZoomIn,
        PlotNavAction::Reset => PlotNavAction::Reset,
    }
}
