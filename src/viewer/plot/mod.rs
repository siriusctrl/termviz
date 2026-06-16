use std::borrow::Cow;

use anyhow::{Context, Result, bail};

use crate::{
    input::InputSource,
    plot::model::PlotScene,
    plot::{PlotKind, stream, table},
    profile::{ContentKind, InputProfile},
    render::Protocol,
    tui::TerminalSession,
};

mod atlas;
mod cache;
mod chrome;
mod events;
mod state;

use cache::PlotFrameCache;
use chrome::{plot_protocol_chrome, render_plot_overlay, status_line_chrome, status_line_text};
use events::{drain_pending_plot_events, handle_plot_event};
use state::PlotViewState;

#[cfg(test)]
use crate::{
    plot::model::PlotBounds,
    render::protocols::{ProtocolRenderContext, render_plot_rgba_with_fallback},
    tui::{ChromeLine, ChromeRole, ChromeSegment, TerminalSize},
};
#[cfg(test)]
use cache::render_plot_frame;
#[cfg(test)]
use chrome::{
    CELL_PIXEL_HEIGHT, CELL_PIXEL_WIDTH, MAX_INTERACTIVE_PLOT_PIXELS, pixel_protocol_target_size,
    plot_protocol_layout,
};
#[cfg(test)]
use crossterm::event::Event;
#[cfg(test)]
use state::PlotNavAction;

pub(crate) fn run(
    source: InputSource,
    profile: InputProfile,
    protocol: Protocol,
    request: PlotRequest,
) -> Result<()> {
    let scene = load_scene(
        &source,
        &profile,
        request.x.as_deref(),
        request.y.as_deref(),
        request.group.as_deref(),
    )?;
    let bounds = scene.bounds().context("plot scene is empty")?.normalized();
    let mut state = PlotViewState::new(bounds);
    let protocol = resolve_plot_protocol(protocol);

    let mut session = TerminalSession::start()?;
    let mut size = session.size().context("reading initial terminal size")?;
    let mut show_overlay = false;
    let mut dirty = true;
    let mut frame_cache = PlotFrameCache::default();
    let mut last_protocol_chrome_size = None;
    let mut recent_action = None;

    loop {
        if dirty {
            let outcome =
                drain_pending_plot_events(&mut session, &mut size, &mut state, &mut show_overlay)?;
            if outcome.quit {
                break;
            }
            let prefetch_action = outcome.action.or(recent_action.take());
            if outcome.resized {
                size = session
                    .size()
                    .context("reading terminal size after resize")?;
            }
            if let Ok(actual_size) = session.size()
                && actual_size != size
            {
                size = actual_size;
            }

            let status_text = status_line_text(size, show_overlay);

            let frame: Cow<'_, str> = if show_overlay {
                Cow::Owned(render_plot_overlay(&scene))
            } else {
                frame_cache.get_or_render(&scene, request.kind, &state, protocol, size)?
            };

            if protocol == Protocol::Blocks || show_overlay {
                session.draw_frame(frame.as_ref(), &status_text)?;
                last_protocol_chrome_size = None;
            } else {
                let repaint_static_chrome = last_protocol_chrome_size != Some(size);
                let chrome = plot_protocol_chrome(
                    frame.as_ref(),
                    &scene,
                    &state,
                    protocol,
                    size,
                    status_line_chrome(show_overlay),
                    repaint_static_chrome,
                );
                session.draw_plot_protocol_frame(chrome)?;
                last_protocol_chrome_size = Some(size);
            }
            if !show_overlay {
                frame_cache.prefetch_neighbors(
                    &scene,
                    request.kind,
                    &state,
                    protocol,
                    size,
                    prefetch_action,
                );
            }
            dirty = false;
        }

        if let Some(event) = session.read_event()? {
            let outcome = handle_plot_event(event, &mut size, &mut state, &mut show_overlay);
            if outcome.quit {
                break;
            }
            recent_action = outcome.action.or(recent_action);
            dirty = dirty || outcome.dirty;
        } else if protocol == Protocol::Kitty && !show_overlay {
            let payloads = frame_cache.drain_transmit_payloads(1);
            session.write_protocol_payloads(&payloads)?;
        }
    }

    Ok(())
}

fn resolve_plot_protocol(protocol: Protocol) -> Protocol {
    match protocol {
        Protocol::Auto => crate::render::terminal::detect(Protocol::Auto).preferred,
        Protocol::Blocks | Protocol::Kitty => protocol,
    }
}

fn load_scene(
    source: &InputSource,
    profile: &InputProfile,
    x: Option<&str>,
    y: Option<&str>,
    group: Option<&str>,
) -> Result<PlotScene> {
    match profile.content {
        ContentKind::Csv => {
            table::load_scene(source, x, y, group, b',').context("loading plot scene from CSV data")
        }
        ContentKind::Tsv => table::load_scene(source, x, y, group, b'\t')
            .context("loading plot scene from TSV data"),
        ContentKind::Jsonl => {
            stream::load_scene(source, x, y, group).context("loading plot scene from jsonl data")
        }
        _ => bail!("plot viewer only supports csv, tsv, and jsonl inputs"),
    }
}

pub(crate) struct PlotRequest {
    pub(crate) x: Option<String>,
    pub(crate) y: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) kind: PlotKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::model::{PlotPoint, PlotSeries};
    use crate::{input::InputSource, profile::InputProfile};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use flate2::read::ZlibDecoder;
    use image::RgbaImage;
    use std::{
        hint::black_box,
        io::{Read, Write},
        path::Path,
        time::{Duration, Instant},
    };

    #[test]
    fn status_line_is_control_only() {
        let size = TerminalSize {
            width: 200,
            height: 24,
        };
        let normal = status_line_text(size, false);
        let overlay = status_line_text(size, true);

        assert!(normal.contains("arrows pan"));
        assert!(normal.contains("+/- zoom"));
        assert!(normal.contains("q quit"));
        assert!(!normal.contains("kitty"));
        assert!(!normal.contains("series"));
        assert!(!normal.contains("pts"));
        assert!(overlay.contains("m chart"));
    }

    #[test]
    fn blocks_plot_frame_does_not_emit_white_background_bands() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Blocks, size).unwrap();

        assert!(!frame.contains("255;255;255"));
        assert!(!frame.contains('▀'));
        assert!(!frame.contains('▄'));
        assert!(!frame.contains('●'));
        assert!(!frame.contains('○'));
        assert!(frame.contains("13;17;23"));
        assert!(frame.contains("38;2;148;163;184"));
        assert!(frame.contains("api"));
        assert!(frame.contains("125.00"));
        assert!(frame.contains("3.000"));
        assert!(contains_braille(&frame));
        assert!(frame.contains('*'));

        let plain = strip_ansi(&frame);
        assert!(
            plain
                .lines()
                .all(|line| line.chars().count() < usize::from(size.width))
        );
    }

    #[test]
    fn image_protocol_plot_frame_renders_raster_payload() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Kitty, size).unwrap();

        assert!(frame.contains("\x1b_G"));
        assert!(!frame.contains("13;17;23"));
        assert!(!contains_braille(&frame));
    }

    #[test]
    fn kitty_plot_frame_encodes_dark_interactive_rgba() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };

        let frame =
            render_plot_frame(&scene, PlotKind::Line, &state, Protocol::Kitty, size).unwrap();
        let image = decode_first_kitty_rgba_payload(&frame);

        assert_eq!(
            (image.width(), image.height()),
            expected_protocol_body_pixels(Protocol::Kitty, size)
        );
        assert_eq!(image.get_pixel(0, 0).0, [13, 17, 23, 255]);
    }

    #[test]
    fn kitty_plot_target_keeps_normal_terminal_size_exact() {
        assert_eq!(
            pixel_protocol_target_size(Protocol::Kitty, 120, 31),
            (960, 496)
        );
    }

    #[test]
    fn kitty_plot_target_caps_large_terminal_pixels() {
        let target = pixel_protocol_target_size(Protocol::Kitty, 300, 99);

        assert!(u64::from(target.0) * u64::from(target.1) <= MAX_INTERACTIVE_PLOT_PIXELS);
        assert!(target.0 < 300 * CELL_PIXEL_WIDTH);
        assert!(target.1 < 99 * CELL_PIXEL_HEIGHT);
    }

    #[test]
    fn plot_frame_cache_reuses_same_payload_for_same_key() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let mut cache = PlotFrameCache::default();

        let first_ptr = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap()
            .as_ptr() as usize;
        let second_ptr = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap()
            .as_ptr() as usize;

        assert_eq!(first_ptr, second_ptr);
        assert_eq!(cache.last.as_ref().unwrap().key.size, size);
    }

    #[test]
    fn plot_frame_cache_rerenders_for_resized_target() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let mut cache = PlotFrameCache::default();

        let small = cache
            .get_or_render(
                &scene,
                PlotKind::Line,
                &state,
                Protocol::Kitty,
                TerminalSize {
                    width: 80,
                    height: 24,
                },
            )
            .unwrap()
            .into_owned();
        let large = cache
            .get_or_render(
                &scene,
                PlotKind::Line,
                &state,
                Protocol::Kitty,
                TerminalSize {
                    width: 120,
                    height: 32,
                },
            )
            .unwrap()
            .into_owned();

        let small_image = decode_first_kitty_rgba_payload(&small);
        let large_image = decode_first_kitty_rgba_payload(&large);

        assert_eq!(
            (small_image.width(), small_image.height()),
            expected_protocol_body_pixels(
                Protocol::Kitty,
                TerminalSize {
                    width: 80,
                    height: 24
                }
            )
        );
        assert_eq!(
            (large_image.width(), large_image.height()),
            expected_protocol_body_pixels(
                Protocol::Kitty,
                TerminalSize {
                    width: 120,
                    height: 32
                }
            )
        );
        assert_eq!(
            cache.last.as_ref().unwrap().key.size,
            TerminalSize {
                width: 120,
                height: 32
            }
        );
    }

    #[test]
    fn plot_frame_cache_places_transmitted_prefetched_pan_frame() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: (0..80)
                    .map(|index| PlotPoint {
                        x: index as f64,
                        y: (index % 17) as f64,
                    })
                    .collect(),
            }],
        };
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let mut state = PlotViewState::new(scene.bounds().unwrap().normalized());
        state.zoom_in();
        let mut cache = PlotFrameCache::default();

        cache.prefetch_neighbors(
            &scene,
            PlotKind::Line,
            &state,
            Protocol::Kitty,
            size,
            Some(PlotNavAction::PanRight),
        );
        let transmit_payloads = wait_for_transmit_payloads(&mut cache, 4);
        assert!(
            transmit_payloads
                .iter()
                .any(|payload| payload.contains("a=t")),
            "prefetch should produce transmit-only kitty payloads"
        );

        state.pan_right();
        let payload = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        assert!(payload.contains("a=p"));
        assert!(!payload.contains("a=T"));
    }

    #[test]
    fn plot_frame_cache_deletes_only_previous_visible_kitty_placement() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let mut state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let mut cache = PlotFrameCache::default();

        let first = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        assert!(first.contains("a=T,i=1"));
        assert!(!first.contains("a=d,d=i"));
        drop(first);

        state.zoom_in();
        let second = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        assert!(second.contains("a=T,i=2"));
        assert!(second.contains("a=d,d=i,i=1,p=1"));
        assert!(!second.contains("a=d,d=Z"));
        assert!(!second.starts_with("\x1b_Ga=d"));
    }

    fn wait_for_transmit_payloads(cache: &mut PlotFrameCache, max_count: usize) -> Vec<String> {
        let mut transmit_payloads = Vec::new();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(25));
            transmit_payloads = cache.drain_transmit_payloads(max_count);
            if !transmit_payloads.is_empty() {
                break;
            }
        }
        transmit_payloads
    }

    #[test]
    fn plot_frame_cache_places_transmitted_prefetched_zoom_then_pan_frame() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: (1..=20)
                    .map(|index| PlotPoint {
                        x: index as f64,
                        y: 118.0 + (index % 9) as f64,
                    })
                    .collect(),
            }],
        };
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let mut state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let mut cache = PlotFrameCache::default();

        let _ = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        state.zoom_in();
        let _ = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        cache.prefetch_neighbors(
            &scene,
            PlotKind::Line,
            &state,
            Protocol::Kitty,
            size,
            Some(PlotNavAction::ZoomIn),
        );
        assert!(
            wait_for_transmit_payloads(&mut cache, 4)
                .iter()
                .any(|payload| payload.contains("a=t"))
        );

        state.zoom_in();
        let payload = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        assert!(payload.contains("a=p"));
        cache.prefetch_neighbors(
            &scene,
            PlotKind::Line,
            &state,
            Protocol::Kitty,
            size,
            Some(PlotNavAction::ZoomIn),
        );

        state.pan_right();
        let _ = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        cache.prefetch_neighbors(
            &scene,
            PlotKind::Line,
            &state,
            Protocol::Kitty,
            size,
            Some(PlotNavAction::PanRight),
        );
        assert!(
            wait_for_transmit_payloads(&mut cache, 4)
                .iter()
                .any(|payload| payload.contains("a=t"))
        );

        state.pan_right();
        let payload = cache
            .get_or_render(&scene, PlotKind::Line, &state, Protocol::Kitty, size)
            .unwrap();
        assert!(payload.contains("a=p"));
        assert!(!payload.contains("a=T"));
    }

    #[test]
    fn resize_event_marks_plot_frame_dirty() {
        let mut size = TerminalSize {
            width: 80,
            height: 24,
        };
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 10.0,
        });
        let mut show_overlay = false;

        let outcome = handle_plot_event(
            Event::Resize(120, 40),
            &mut size,
            &mut state,
            &mut show_overlay,
        );

        assert!(outcome.dirty);
        assert!(outcome.resized);
        assert_eq!(
            size,
            TerminalSize {
                width: 120,
                height: 40
            }
        );
    }

    #[test]
    fn plot_frame_renders_every_explicit_protocol() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "api".to_owned(),
                points: vec![
                    PlotPoint { x: 1.0, y: 118.0 },
                    PlotPoint { x: 2.0, y: 121.0 },
                    PlotPoint { x: 3.0, y: 125.0 },
                ],
            }],
        };
        let state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let size = TerminalSize {
            width: 80,
            height: 24,
        };
        let cases = [(Protocol::Blocks, "13;17;23"), (Protocol::Kitty, "\x1b_G")];

        for (protocol, marker) in cases {
            let frame = render_plot_frame(&scene, PlotKind::Line, &state, protocol, size)
                .unwrap_or_else(|error| panic!("{protocol:?} plot frame failed: {error}"));

            assert!(
                frame.contains(marker),
                "{protocol:?} plot frame did not include marker {marker:?}"
            );
        }
    }

    #[test]
    fn plot_protocol_chrome_promotes_labels_outside_image_payload() {
        let scene = PlotScene {
            title: Some("latency.csv".to_owned()),
            series: vec![
                PlotSeries {
                    name: "api".to_owned(),
                    points: vec![PlotPoint { x: 1.0, y: 118.0 }],
                },
                PlotSeries {
                    name: "worker".to_owned(),
                    points: vec![PlotPoint { x: 2.0, y: 134.0 }],
                },
            ],
        };
        let state = PlotViewState::new(PlotBounds {
            x_min: 1.0,
            x_max: 20.0,
            y_min: 118.0,
            y_max: 205.0,
        });
        let size = TerminalSize {
            width: 120,
            height: 32,
        };
        let chrome = plot_protocol_chrome(
            "payload",
            &scene,
            &state,
            Protocol::Kitty,
            size,
            ChromeLine::new(vec![ChromeSegment::new("status", ChromeRole::Muted)]),
            true,
        );

        assert_eq!(chrome.body.col, 9);
        assert_eq!(chrome.body.row, 2);
        assert_eq!(chrome.body.cols, 111);
        assert_eq!(chrome.body.rows, 28);
        assert_eq!(chrome.chrome.static_layer.x_axis_row, 30);
        assert!(chrome.chrome.static_layer.repaint);
        assert!(
            chrome
                .chrome
                .dynamic_layer
                .header
                .segments
                .iter()
                .any(|segment| segment.text == "latency.csv" && segment.role == ChromeRole::Title)
        );
        assert!(
            chrome
                .chrome
                .dynamic_layer
                .header
                .segments
                .iter()
                .any(|segment| segment.text == "kitty" && segment.role == ChromeRole::Meta)
        );
        assert_eq!(chrome.chrome.static_layer.legend.len(), 2);
        assert!(
            chrome
                .chrome
                .static_layer
                .legend
                .iter()
                .all(|item| item.marker == "━━")
        );
        assert!(
            chrome
                .chrome
                .dynamic_layer
                .y_labels
                .iter()
                .any(|label| label.text.contains("205"))
        );
        assert!(
            chrome
                .chrome
                .dynamic_layer
                .x_labels
                .iter()
                .any(|label| label.text.contains("20"))
        );
    }

    #[test]
    #[ignore = "local perf test; run scripts/bench-plot-recompute.sh"]
    fn plot_recompute_perf() {
        let iterations = std::env::var("TERMVIZ_PLOT_RECOMPUTE_ITERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(12);
        let scene = dense_perf_scene();
        let base_state = PlotViewState::new(scene.bounds().unwrap().normalized());
        let small_size = TerminalSize {
            width: 80,
            height: 24,
        };
        let large_size = TerminalSize {
            width: 120,
            height: 32,
        };

        print_detailed_perf_metric("kitty_uncached_120x32", iterations, || {
            vec![
                profile_pixel_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile kitty plot frame"),
            ]
        });

        print_detailed_perf_metric("kitty_resize_80x24_120x32", iterations, || {
            let small = profile_pixel_plot_frame(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                small_size,
            )
            .expect("profile small kitty plot frame");
            let large = profile_pixel_plot_frame(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                large_size,
            )
            .expect("profile large kitty plot frame");
            vec![small, large]
        });

        let mut cache = PlotFrameCache::default();
        let _ = cache
            .get_or_render(
                black_box(&scene),
                PlotKind::Line,
                black_box(&base_state),
                Protocol::Kitty,
                large_size,
            )
            .expect("prime plot frame cache");
        print_detailed_perf_metric("kitty_cache_hit_120x32", iterations, || {
            vec![
                profile_cached_plot_frame(
                    &mut cache,
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile plot frame cache hit"),
            ]
        });

        print_detailed_perf_metric("kitty_pan_burst_120x32", iterations, || {
            let mut state = base_state;
            let mut samples = Vec::new();
            state.zoom_in();
            for _ in 0..8 {
                state.pan_right();
                let sample = profile_pixel_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&state),
                    Protocol::Kitty,
                    large_size,
                )
                .expect("profile panned kitty plot frame");
                samples.push(sample);
            }
            samples
        });

        print_detailed_perf_metric("blocks_uncached_120x32", iterations, || {
            vec![
                profile_blocks_plot_frame(
                    black_box(&scene),
                    PlotKind::Line,
                    black_box(&base_state),
                    Protocol::Blocks,
                    large_size,
                )
                .expect("profile blocks plot frame"),
            ]
        });

        let mut file = tempfile::NamedTempFile::with_suffix(".csv").unwrap();
        write_dense_perf_csv(&mut file);
        print_detailed_perf_metric("kitty_full_pipeline_120x32", iterations, || {
            vec![
                profile_plot_startup_and_frame(file.path(), Protocol::Kitty, large_size)
                    .expect("profile full kitty plot pipeline"),
            ]
        });
        print_detailed_perf_metric("blocks_full_pipeline_120x32", iterations, || {
            vec![
                profile_plot_startup_and_frame(file.path(), Protocol::Blocks, large_size)
                    .expect("profile full blocks plot pipeline"),
            ]
        });
    }

    #[test]
    fn plot_overlay_contains_point_series_and_bounds() {
        let scene = PlotScene {
            title: Some("latency".to_owned()),
            series: vec![PlotSeries {
                name: "svc-a".to_owned(),
                points: vec![PlotPoint { x: 1.0, y: 20.0 }, PlotPoint { x: 2.0, y: 40.0 }],
            }],
        };

        let overlay = render_plot_overlay(&scene);
        assert!(overlay.contains("points: 2"));
        assert!(overlay.contains("series: 1"));
        assert!(overlay.contains("x: [1.000, 2.000]"));
        assert!(overlay.contains("y: [20.000, 40.000]"));
    }

    #[test]
    fn plot_view_state_can_pan_and_zoom() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });
        assert_eq!(state.visible.x_min, 0.0);
        assert!(state.fit_mode);

        state.zoom_in();
        assert!(!state.fit_mode);
        state.pan_right();
        assert!(!state.fit_mode);
        assert!(state.visible.x_min > 0.0);

        let current_span_x = state.visible.x_max - state.visible.x_min;
        state.zoom_in();
        assert!(state.visible.x_max - state.visible.x_min < current_span_x);

        state.reset();
        assert!(state.fit_mode);
        assert_eq!(state.visible.x_min, 0.0);
        assert_eq!(state.visible.x_max, 100.0);
    }

    #[derive(Debug, Clone)]
    struct PlotPerfSample {
        total: Duration,
        profile: Duration,
        load: Duration,
        layout: Duration,
        display_list: Duration,
        raster: Duration,
        compose: Duration,
        protocol: Duration,
        chrome: Duration,
        bytes: usize,
        chrome_bytes: usize,
        commands: usize,
        image_pixels: u64,
    }

    fn profile_pixel_plot_frame(
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        assert_ne!(protocol, Protocol::Blocks);
        let total_start = Instant::now();
        let layout_start = Instant::now();
        let layout = plot_protocol_layout(size);
        let target = pixel_protocol_target_size(
            protocol,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        );
        let layout_time = layout_start.elapsed();

        let timed = crate::render::protocols::plot::render_interactive_plot_body_timed_for_size(
            scene,
            kind,
            state.visible,
            target.0,
            target.1,
        )?;

        let protocol_start = Instant::now();
        let payload = render_plot_rgba_with_fallback(
            ProtocolRenderContext::new(protocol),
            &timed.image,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        );
        let protocol_time = protocol_start.elapsed();

        let chrome_start = Instant::now();
        let chrome = plot_protocol_chrome(
            &payload,
            scene,
            state,
            protocol,
            size,
            status_line_chrome(false),
            false,
        );
        let chrome_bytes = estimate_plot_chrome_bytes(&chrome, size);
        let chrome_time = chrome_start.elapsed();

        Ok(PlotPerfSample {
            total: total_start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: layout_time,
            display_list: timed.display_list,
            raster: timed.raster,
            compose: Duration::ZERO,
            protocol: protocol_time,
            chrome: chrome_time,
            bytes: payload.len(),
            chrome_bytes,
            commands: timed.command_count,
            image_pixels: u64::from(timed.image.width()) * u64::from(timed.image.height()),
        })
    }

    fn profile_blocks_plot_frame(
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        assert_eq!(protocol, Protocol::Blocks);
        let start = Instant::now();
        let payload = render_plot_frame(scene, kind, state, protocol, size)?;
        Ok(PlotPerfSample {
            total: start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: Duration::ZERO,
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            compose: Duration::ZERO,
            protocol: Duration::ZERO,
            chrome: Duration::ZERO,
            bytes: payload.len(),
            chrome_bytes: 0,
            commands: 0,
            image_pixels: 0,
        })
    }

    fn profile_plot_startup_and_frame(
        path: &Path,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        let total_start = Instant::now();

        let profile_start = Instant::now();
        let source = InputSource::from_path(path.to_path_buf())?;
        let profile = InputProfile::resolve(&source, PlotKind::Line, None)?;
        let profile_time = profile_start.elapsed();

        let load_start = Instant::now();
        let scene = load_scene(
            &source,
            &profile,
            Some("time"),
            Some("latency"),
            Some("service"),
        )?;
        let load_time = load_start.elapsed();

        let state = PlotViewState::new(scene.bounds().context("plot scene is empty")?.normalized());
        let mut sample = match protocol {
            Protocol::Kitty => {
                profile_pixel_plot_frame(&scene, PlotKind::Line, &state, protocol, size)?
            }
            Protocol::Blocks => {
                profile_blocks_plot_frame(&scene, PlotKind::Line, &state, protocol, size)?
            }
            Protocol::Auto => unreachable!("auto protocol should be resolved before profiling"),
        };
        sample.profile = profile_time;
        sample.load = load_time;
        sample.total = total_start.elapsed();
        Ok(sample)
    }

    fn profile_cached_plot_frame(
        cache: &mut PlotFrameCache,
        scene: &PlotScene,
        kind: PlotKind,
        state: &PlotViewState,
        protocol: Protocol,
        size: TerminalSize,
    ) -> Result<PlotPerfSample> {
        let start = Instant::now();
        let payload = cache.get_or_render(scene, kind, state, protocol, size)?;
        Ok(PlotPerfSample {
            total: start.elapsed(),
            profile: Duration::ZERO,
            load: Duration::ZERO,
            layout: Duration::ZERO,
            display_list: Duration::ZERO,
            raster: Duration::ZERO,
            compose: Duration::ZERO,
            protocol: Duration::ZERO,
            chrome: Duration::ZERO,
            bytes: payload.len(),
            chrome_bytes: 0,
            commands: 0,
            image_pixels: 0,
        })
    }

    fn print_detailed_perf_metric<F>(name: &str, iterations: usize, mut run: F)
    where
        F: FnMut() -> Vec<PlotPerfSample>,
    {
        let mut total_time = Duration::ZERO;
        let mut profile_time = Duration::ZERO;
        let mut load_time = Duration::ZERO;
        let mut layout_time = Duration::ZERO;
        let mut display_list_time = Duration::ZERO;
        let mut raster_time = Duration::ZERO;
        let mut compose_time = Duration::ZERO;
        let mut protocol_time = Duration::ZERO;
        let mut chrome_time = Duration::ZERO;
        let mut bytes = 0usize;
        let mut chrome_bytes = 0usize;
        let mut commands = 0usize;
        let mut image_pixels = 0u64;
        for _ in 0..iterations {
            for sample in run() {
                total_time += sample.total;
                profile_time += sample.profile;
                load_time += sample.load;
                layout_time += sample.layout;
                display_list_time += sample.display_list;
                raster_time += sample.raster;
                compose_time += sample.compose;
                protocol_time += sample.protocol;
                chrome_time += sample.chrome;
                bytes = bytes.saturating_add(black_box(sample.bytes));
                chrome_bytes = chrome_bytes.saturating_add(black_box(sample.chrome_bytes));
                commands = commands.saturating_add(black_box(sample.commands));
                image_pixels = image_pixels.saturating_add(black_box(sample.image_pixels));
            }
        }

        let total_us = total_time.as_micros();
        let mean_us = total_us / iterations as u128;
        let mean_profile_us = profile_time.as_micros() / iterations as u128;
        let mean_load_us = load_time.as_micros() / iterations as u128;
        let mean_layout_us = layout_time.as_micros() / iterations as u128;
        let mean_display_list_us = display_list_time.as_micros() / iterations as u128;
        let mean_raster_us = raster_time.as_micros() / iterations as u128;
        let mean_compose_us = compose_time.as_micros() / iterations as u128;
        let mean_protocol_us = protocol_time.as_micros() / iterations as u128;
        let mean_chrome_us = chrome_time.as_micros() / iterations as u128;
        let mean_bytes = bytes / iterations;
        let mean_chrome_bytes = chrome_bytes / iterations;
        let mean_commands = commands / iterations;
        let mean_image_pixels = image_pixels / iterations as u64;
        if !name.contains("full_pipeline") {
            println!(
                "plot_recompute_detail,{name},{iterations},{total_us},{mean_us},{mean_display_list_us},{mean_raster_us},{mean_protocol_us},{bytes},{mean_bytes},{mean_commands},{mean_image_pixels}"
            );
        }
        println!(
            "plot_pipeline_detail,{name},{iterations},{total_us},{mean_us},{mean_profile_us},{mean_load_us},{mean_layout_us},{mean_display_list_us},{mean_raster_us},{mean_compose_us},{mean_protocol_us},{mean_chrome_us},{bytes},{mean_bytes},{mean_chrome_bytes},{mean_commands},{mean_image_pixels}"
        );
    }

    fn expected_protocol_body_pixels(protocol: Protocol, size: TerminalSize) -> (u32, u32) {
        let layout = plot_protocol_layout(size);
        pixel_protocol_target_size(
            protocol,
            u32::from(layout.image_cols),
            u32::from(layout.image_rows),
        )
    }

    fn dense_perf_scene() -> PlotScene {
        let series = (0..4)
            .map(|series_index| {
                let points = (0..256)
                    .map(|index| {
                        let x = index as f64;
                        let wave = ((index as f64 + series_index as f64 * 13.0) / 15.0).sin();
                        let trend = index as f64 * (0.12 + series_index as f64 * 0.02);
                        PlotPoint {
                            x,
                            y: 120.0 + trend + wave * (8.0 + series_index as f64 * 3.0),
                        }
                    })
                    .collect();
                PlotSeries {
                    name: format!("svc-{series_index}"),
                    points,
                }
            })
            .collect();

        PlotScene {
            title: Some("perf".to_owned()),
            series,
        }
    }

    fn write_dense_perf_csv(file: &mut tempfile::NamedTempFile) {
        writeln!(file, "time,latency,service").unwrap();
        for point in 0..1_024 {
            let x = point as f64;
            let api = 150.0 + (x / 8.0).sin() * 42.0 + (x / 29.0).cos() * 7.0;
            let worker = 132.0 + (x / 11.0).cos() * 30.0 + (x / 35.0).sin() * 11.0;
            writeln!(file, "{x:.3},{api:.3},api").unwrap();
            writeln!(file, "{x:.3},{worker:.3},worker").unwrap();
        }
    }

    fn estimate_plot_chrome_bytes(
        frame: &crate::tui::PlotProtocolFrame<'_>,
        size: TerminalSize,
    ) -> usize {
        let legend_bytes = if frame.chrome.static_layer.repaint {
            frame
                .chrome
                .static_layer
                .legend
                .iter()
                .map(|item| item.marker.len() + item.label.len() + 3)
                .sum::<usize>()
        } else {
            0
        };
        let text_bytes = chrome_line_bytes(&frame.chrome.dynamic_layer.header)
            + chrome_line_bytes(&frame.chrome.dynamic_layer.status)
            + legend_bytes
            + frame
                .chrome
                .dynamic_layer
                .y_labels
                .iter()
                .chain(frame.chrome.dynamic_layer.x_labels.iter())
                .map(|label| label.text.len())
                .sum::<usize>();
        let background_cells = if frame.chrome.static_layer.repaint {
            usize::from(frame.body.row) * usize::from(size.width)
                + usize::from(frame.body.col) * usize::from(frame.body.rows)
                + usize::from(size.width)
        } else {
            0
        };
        text_bytes + background_cells
    }

    fn chrome_line_bytes(line: &ChromeLine) -> usize {
        line.segments.iter().map(|segment| segment.text.len()).sum()
    }

    #[test]
    fn plot_view_state_keeps_fit_when_pan_has_no_room() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });

        state.pan_right();
        state.pan_up();

        assert!(state.fit_mode);
        assert_eq!(state.visible, state.full);
    }

    #[test]
    fn plot_view_state_vertical_pan_matches_data_direction() {
        let mut state = PlotViewState::new(PlotBounds {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 100.0,
        });
        state.zoom_in();
        let after_zoom = state.visible;

        state.pan_up();
        assert!(state.visible.y_min > after_zoom.y_min);

        let after_pan_up = state.visible;
        state.pan_down();
        assert!(state.visible.y_min < after_pan_up.y_min);
    }

    fn strip_ansi(text: &str) -> String {
        let mut output = String::new();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            output.push(ch);
        }
        output
    }

    fn contains_braille(text: &str) -> bool {
        text.chars()
            .any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))
    }

    fn decode_first_kitty_rgba_payload(payload: &str) -> RgbaImage {
        let mut base64_payload = String::new();
        let mut first_control = None;
        for packet in payload.split("\x1b_G") {
            let Some(packet) = packet.strip_suffix("\x1b\\") else {
                continue;
            };
            let Some((control, chunk)) = packet.split_once(';') else {
                continue;
            };
            if chunk.is_empty() {
                continue;
            }
            if control.contains("t=f") {
                let path = String::from_utf8(STANDARD.decode(chunk).unwrap()).unwrap();
                return image::load_from_memory(&std::fs::read(path).unwrap())
                    .unwrap()
                    .to_rgba8();
            }
            first_control.get_or_insert_with(|| control.to_owned());
            base64_payload.push_str(chunk);
        }
        assert!(!base64_payload.is_empty());
        let control = first_control.expect("kitty payload should include control data");
        let decoded = STANDARD.decode(base64_payload).unwrap();
        if control.contains("f=100") {
            return image::load_from_memory(&decoded).unwrap().to_rgba8();
        }

        let bytes_per_pixel = if control.contains("f=24") {
            3
        } else if control.contains("f=32") {
            4
        } else {
            panic!("expected RGB, RGBA, or PNG kitty payload, got {control}");
        };
        let width = parse_kitty_control_u32(&control, "s").expect("kitty payload width");
        let height = parse_kitty_control_u32(&control, "v").expect("kitty payload height");
        let pixels = if control.contains("o=z") {
            let mut decoder = ZlibDecoder::new(decoded.as_slice());
            let mut output = Vec::new();
            decoder.read_to_end(&mut output).unwrap();
            output
        } else {
            decoded
        };
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * bytes_per_pixel
        );
        let rgba = if bytes_per_pixel == 3 {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in pixels.chunks_exact(3) {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
            rgba
        } else {
            pixels
        };
        RgbaImage::from_raw(width, height, rgba).expect("valid RGBA payload")
    }

    fn parse_kitty_control_u32(control: &str, key: &str) -> Option<u32> {
        control.split(',').find_map(|part| {
            let (candidate, value) = part.split_once('=')?;
            (candidate == key).then(|| value.parse().ok()).flatten()
        })
    }
}
