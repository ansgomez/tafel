// Imports
use super::ToolsState;
use crate::engine::{EngineView, EngineViewMut};
use crate::{DrawableOnDoc, WidgetFlags};
use p2d::bounding_volume::Aabb;
use p2d::math::Vector2;
use piet::RenderContext;
use rnote_compose::eventresult::{EventPropagation, EventResult};
use rnote_compose::ext::{AabbExt, Vector2Ext};
use rnote_compose::penevent::{PenEvent, PenProgress};
use std::time::Duration;
use std::time::Instant;

/// A laser pointer that leaves a short, tapered and fading comet trail.
///
/// Every point fades out a short time after it was added, so the trail follows the pen
/// and disappears quickly once the pen lifts. Nothing is ever stored in the document.
#[derive(Debug, Default)]
pub struct LaserTool {
    state: ToolsState,
    /// One trail per pen down, each with its points and the time they were added.
    trails: Vec<Vec<(Vector2, Instant)>>,
}

impl LaserTool {
    /// A point fades out after this long.
    const POINT_LIFETIME: Duration = Duration::from_millis(350);
    /// The longest trail, in screen pixels.
    const TRAIL_MAX_LEN: f64 = 140.0;
    /// The trail width at the head, in screen pixels. It tapers to zero at the tail.
    const TRAIL_WIDTH: f64 = 8.0;
    /// The radius of the dot at the head, in screen pixels.
    const DOT_RADIUS: f64 = 7.0;
    /// The radius of the glow around the dot, in screen pixels.
    const GLOW_RADIUS: f64 = 13.0;

    const COLOR: piet::Color = piet::Color::rgb8(0xff, 0x1f, 0x1f);

    fn add_point(&mut self, pos: Vector2, now: Instant, engine_view: &mut EngineViewMut) {
        if let Some(trail) = self.trails.last_mut() {
            trail.push((pos, now));
        }
        engine_view.animation.claim_frame();
    }

    pub fn handle_event(
        &mut self,
        event: PenEvent,
        now: Instant,
        engine_view: &mut EngineViewMut,
    ) -> (EventResult<PenProgress>, WidgetFlags) {
        let widget_flags = WidgetFlags::default();

        let event_result = match (&mut self.state, event) {
            (ToolsState::Idle, PenEvent::Down { element, .. }) => {
                self.trails.push(Vec::new());
                self.add_point(element.pos, now, engine_view);
                self.state = ToolsState::Active;

                EventResult {
                    handled: true,
                    propagate: EventPropagation::Stop,
                    progress: PenProgress::InProgress,
                }
            }
            (ToolsState::Idle, _) => EventResult {
                handled: false,
                propagate: EventPropagation::Proceed,
                progress: PenProgress::Idle,
            },
            (ToolsState::Active, PenEvent::Down { element, .. }) => {
                self.add_point(element.pos, now, engine_view);

                EventResult {
                    handled: true,
                    propagate: EventPropagation::Stop,
                    progress: PenProgress::InProgress,
                }
            }
            (ToolsState::Active, PenEvent::Up { element, .. }) => {
                self.add_point(element.pos, now, engine_view);
                self.state = ToolsState::Idle;

                EventResult {
                    handled: true,
                    propagate: EventPropagation::Stop,
                    progress: PenProgress::Finished,
                }
            }
            (ToolsState::Active, PenEvent::Cancel) => {
                self.state = ToolsState::Idle;
                self.trails.clear();

                EventResult {
                    handled: true,
                    propagate: EventPropagation::Stop,
                    progress: PenProgress::Finished,
                }
            }
            (
                ToolsState::Active,
                PenEvent::Proximity { .. } | PenEvent::KeyPressed { .. } | PenEvent::Text { .. },
            ) => EventResult {
                handled: false,
                propagate: EventPropagation::Proceed,
                progress: PenProgress::InProgress,
            },
        };

        (event_result, widget_flags)
    }

    pub fn handle_animation_frame(&mut self, engine_view: &mut EngineViewMut) {
        let now = Instant::now();
        let active = self.state == ToolsState::Active;
        let n_trails = self.trails.len();
        for (i, trail) in self.trails.iter_mut().enumerate() {
            // Keep the newest point of the trail that is still being drawn, so the dot
            // stays under a pen that rests on the surface.
            let keep_head = active && i + 1 == n_trails;
            let len = trail.len();
            let mut idx = 0;
            trail.retain(|(_, time)| {
                idx += 1;
                (keep_head && idx == len) || now.duration_since(*time) < Self::POINT_LIFETIME
            });
        }
        self.trails.retain(|trail| !trail.is_empty());

        // Once everything has faded, the frame requested last still draws the cleared trail.
        if !self.trails.is_empty() {
            engine_view.animation.claim_frame();
        }
    }

    /// The opacity of a trail: it fades out with the age of its newest point,
    /// except while it is still being drawn.
    fn trail_opacity(&self, trail: &[(Vector2, Instant)], now: Instant, is_newest: bool) -> f64 {
        if is_newest && self.state == ToolsState::Active {
            return 1.0;
        }
        let Some((_, head_time)) = trail.last() else {
            return 0.0;
        };
        (1.0 - now
            .duration_since(*head_time)
            .div_duration_f64(Self::POINT_LIFETIME))
        .clamp(0.0, 1.0)
    }

    /// The outline of a trail that is cut to `max_len` and tapers from `width` at the head to
    /// zero at the tail.
    fn trail_outline(points: &[Vector2], max_len: f64, width: f64) -> Option<kurbo::BezPath> {
        // Walk from the head to the tail, until the maximum length is reached.
        let mut line = vec![*points.last()?];
        let mut len = 0.0;
        for p in points.iter().rev().skip(1) {
            let last = *line.last().unwrap();
            let step = (*p - last).length();
            if step < f64::EPSILON {
                continue;
            }
            if len + step > max_len {
                line.push(last + (*p - last) * ((max_len - len) / step));
                len = max_len;
                break;
            }
            len += step;
            line.push(*p);
        }
        if line.len() < 2 || len < f64::EPSILON {
            return None;
        }
        line.reverse();

        // Offset both sides by half the width, which grows from the tail to the head.
        let mut left = Vec::with_capacity(line.len());
        let mut right = Vec::with_capacity(line.len());
        let mut acc = 0.0;
        for i in 0..line.len() {
            if i > 0 {
                acc += (line[i] - line[i - 1]).length();
            }
            let prev = line[i.saturating_sub(1)];
            let next = line[(i + 1).min(line.len() - 1)];
            let normal = (next - prev).orth_unit();
            let half_width = 0.5 * width * acc / len;
            left.push(line[i] + normal * half_width);
            right.push(line[i] - normal * half_width);
        }

        let mut path = kurbo::BezPath::new();
        path.move_to(left[0].to_kurbo_point());
        for p in left.iter().skip(1).chain(right.iter().rev()) {
            path.line_to(p.to_kurbo_point());
        }
        path.close_path();
        Some(path)
    }
}

impl DrawableOnDoc for LaserTool {
    fn bounds_on_doc(&self, engine_view: &EngineView) -> Option<Aabb> {
        let mut points = self.trails.iter().flatten().map(|(pos, _)| *pos);
        let first = points.next()?;
        let (mins, maxs) = points.fold((first, first), |(mins, maxs), p| {
            (mins.mins(&p), maxs.maxs(&p))
        });

        Some(Aabb::new(mins, maxs).extend_by(Vector2::splat(
            Self::GLOW_RADIUS / engine_view.camera.total_zoom(),
        )))
    }

    fn draw_on_doc(
        &self,
        cx: &mut piet_cairo::CairoRenderContext,
        engine_view: &EngineView,
    ) -> anyhow::Result<()> {
        cx.save().map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let now = Instant::now();
        let total_zoom = engine_view.camera.total_zoom();
        let n_trails = self.trails.len();

        for (i, trail) in self.trails.iter().enumerate() {
            let opacity = self.trail_opacity(trail, now, i + 1 == n_trails);
            if opacity <= 0.0 {
                continue;
            }
            let color = Self::COLOR.with_alpha(opacity);
            let points = trail.iter().map(|(pos, _)| *pos).collect::<Vec<Vector2>>();

            // One filled shape instead of separate segments, so there are no beads or gaps.
            if let Some(outline) = Self::trail_outline(
                &points,
                Self::TRAIL_MAX_LEN / total_zoom,
                Self::TRAIL_WIDTH / total_zoom,
            ) {
                cx.fill(outline, &color);
            }

            // The dot at the head, with a soft glow around it.
            let Some(head) = points.last().map(|head| head.to_kurbo_point()) else {
                continue;
            };
            cx.fill(
                kurbo::Circle::new(head, Self::GLOW_RADIUS / total_zoom),
                &Self::COLOR.with_alpha(0.15 * opacity),
            );
            cx.fill(
                kurbo::Circle::new(head, (Self::DOT_RADIUS + 2.5) / total_zoom),
                &Self::COLOR.with_alpha(0.3 * opacity),
            );
            cx.fill(
                kurbo::Circle::new(head, Self::DOT_RADIUS / total_zoom),
                &color,
            );
        }

        cx.restore().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok(())
    }
}
