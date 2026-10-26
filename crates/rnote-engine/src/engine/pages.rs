// Imports
use super::Engine;
use crate::document::Layout;
use crate::store::StrokeKey;
use crate::store::chrono_comp::StrokeLayer;
use crate::strokes::resize::ImageSizeOption;
use crate::strokes::{Stroke, VectorImage};
use crate::{Camera, WidgetFlags};
use p2d::bounding_volume::Aabb;
use p2d::math::Vector2;
use rnote_compose::shapes::Shapeable;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// The paper of a blank page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlankPaper {
    #[default]
    Plain,
    Lined,
    Grid,
}

impl BlankPaper {
    // These colors are only used by blank page paper, so its strokes can be recognized.
    const BASE_COLOR: &'static str = "#fffffe";
    const RULE_COLOR: &'static str = "#a9c8e9";
    const GRID_COLOR: &'static str = "#d5dbe4";
    const MARGIN_COLOR: &'static str = "#e9a3ae";

    /// Svg of the paper for a page of the given size: white, with rules or a grid of `size.x / 32`
    /// spacing.
    fn svg(self, size: Vector2) -> String {
        let (w, h) = (size.x, size.y);
        let step = w / 32.0;
        let mut content = format!(
            r#"<rect x="0" y="0" width="{w}" height="{h}" fill="{}"/>"#,
            Self::BASE_COLOR
        );
        match self {
            BlankPaper::Plain => {}
            BlankPaper::Lined => {
                let mut y = 2.0 * step - 0.5;
                while y < h {
                    content.push_str(&format!(
                        r#"<line x1="0" y1="{y}" x2="{w}" y2="{y}" stroke="{}" stroke-width="1.2"/>"#,
                        Self::RULE_COLOR
                    ));
                    y += step;
                }
                content.push_str(&format!(
                    r#"<line x1="{x}" y1="0" x2="{x}" y2="{h}" stroke="{}" stroke-width="1.2"/>"#,
                    Self::MARGIN_COLOR,
                    x = 3.0 * step
                ));
            }
            BlankPaper::Grid => {
                let mut x = step;
                while x < w {
                    content.push_str(&format!(
                        r#"<line x1="{x}" y1="0" x2="{x}" y2="{h}" stroke="{}" stroke-width="1"/>"#,
                        Self::GRID_COLOR
                    ));
                    x += step;
                }
                let mut y = step;
                while y < h {
                    content.push_str(&format!(
                        r#"<line x1="0" y1="{y}" x2="{w}" y2="{y}" stroke="{}" stroke-width="1"/>"#,
                        Self::GRID_COLOR
                    ));
                    y += step;
                }
            }
        }
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{content}</svg>"#
        )
    }

    /// The paper of a stroke, if it is the paper of a blank page.
    ///
    /// Paper consists only of the white page area and lines in the paper colors, so a Pdf page
    /// that happens to use one of the colors is not taken for paper.
    fn of_stroke(stroke: &Stroke) -> Option<Self> {
        let Stroke::VectorImage(image) = stroke else {
            return None;
        };
        let svg = image.svg_data.as_str();
        let attr = |element: &str, name: &str| -> Option<String> {
            let start = element.find(&format!(" {name}=\""))? + name.len() + 3;
            let len = element[start..].find('"')?;
            Some(element[start..start + len].to_string())
        };
        let line_colors = [Self::RULE_COLOR, Self::GRID_COLOR, Self::MARGIN_COLOR];
        let mut elements = svg
            .split('<')
            .filter(|e| !e.is_empty() && !e.starts_with('/'));
        let only_paper = elements.all(|element| {
            if element.starts_with("svg ") || element.starts_with("g>") || element.starts_with("g ")
            {
                true
            } else if element.starts_with("path ") {
                let fill = attr(element, "fill");
                let stroke = attr(element, "stroke");
                (fill.as_deref() == Some(Self::BASE_COLOR) && stroke.as_deref() == Some("none"))
                    || stroke.is_some_and(|stroke| line_colors.contains(&stroke.as_str()))
            } else {
                false
            }
        });
        if !svg.contains(Self::BASE_COLOR) || !only_paper {
            None
        } else if image.svg_data.contains(Self::RULE_COLOR) {
            Some(Self::Lined)
        } else if image.svg_data.contains(Self::GRID_COLOR) {
            Some(Self::Grid)
        } else {
            Some(Self::Plain)
        }
    }
}

/// What the page counter shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageLabel {
    /// The number of the current content page (counting from 1), or of the content page before the
    /// current blank page.
    pub page: u32,
    /// The number of content pages. Pages are content pages unless they are blank.
    pub n_pages: u32,
    /// The number of blank pages that follow the content page.
    pub n_blank: u32,
    /// The position of the current page among those blank pages (counting from 1), 0 when on the
    /// content page.
    pub blank: u32,
}

impl Engine {
    /// Strokes that start this close above a page border still count as being on the next page.
    const PAGE_BORDER_TOLERANCE: f64 = 0.5;

    fn format_height(&self) -> Option<f64> {
        let format_height = self.document.config.format.height();
        (format_height > 0.0).then_some(format_height)
    }

    /// Whether a single page is shown at a time (in the fixed size layout).
    pub fn doc_single_page(&self) -> bool {
        self.document.single_page_bounds().is_some()
    }

    /// The number of pages in the document, by its format height.
    pub fn doc_n_pages(&self) -> u32 {
        self.document.n_pages()
    }

    /// The shown page in the single page view, otherwise the page at the center of the viewport.
    pub fn doc_current_page(&self) -> u32 {
        let n_pages = self.doc_n_pages();
        if self.document.single_page_bounds().is_some() {
            return self.document.shown_page.min(n_pages.saturating_sub(1));
        }
        let Some(format_height) = self.format_height() else {
            return 0;
        };
        let center_y = self.camera.viewport_center().y - self.document.y;
        ((center_y / format_height).floor().max(0.0) as u32).min(n_pages.saturating_sub(1))
    }

    /// The bounds of the page with the given index, shrunk a bit so strokes touching the
    /// neighboring pages do not count.
    fn page_inner_bounds(&self, page: u32) -> Option<Aabb> {
        let format_height = self.format_height()?;
        let top = self.document.y + page as f64 * format_height;
        Some(Aabb::new(
            Vector2::new(self.document.x, top + Self::PAGE_BORDER_TOLERANCE),
            Vector2::new(
                self.document.x + self.document.width,
                top + format_height - Self::PAGE_BORDER_TOLERANCE,
            ),
        ))
    }

    fn page_keys(&self, page: u32) -> Vec<StrokeKey> {
        self.page_inner_bounds(page)
            .map(|bounds| {
                self.store
                    .stroke_keys_as_rendered_intersecting_bounds(bounds)
            })
            .unwrap_or_default()
    }

    /// The strokes on a page that start on it, so strokes that only reach into it from the page
    /// above are not included.
    fn page_own_keys(&self, page: u32) -> Vec<StrokeKey> {
        let Some(format_height) = self.format_height() else {
            return Vec::new();
        };
        let page_top = self.document.y + page as f64 * format_height;
        self.page_keys(page)
            .into_iter()
            .filter(|&key| {
                self.store.get_stroke_ref(key).is_some_and(|stroke| {
                    stroke.bounds().mins.y >= page_top - Self::PAGE_BORDER_TOLERANCE
                })
            })
            .collect()
    }

    /// Whether the page shows document content (an imported page), or is a blank page.
    fn page_is_content(&self, page: u32) -> bool {
        self.page_keys(page).into_iter().any(|key| {
            self.store.stroke_layer(key) == Some(StrokeLayer::Document)
                && self
                    .store
                    .get_stroke_ref(key)
                    .is_some_and(|stroke| BlankPaper::of_stroke(stroke).is_none())
        })
    }

    /// Whether the current page is a blank page (one without document content).
    pub fn doc_current_page_is_blank(&self) -> bool {
        self.doc_n_pages() > 0 && !self.page_is_content(self.doc_current_page())
    }

    /// The paper of the current page, if it is a blank page.
    pub fn doc_current_page_paper(&self) -> Option<BlankPaper> {
        if !self.doc_current_page_is_blank() {
            return None;
        }
        Some(
            self.page_keys(self.doc_current_page())
                .into_iter()
                .find_map(|key| {
                    self.store
                        .get_stroke_ref(key)
                        .and_then(BlankPaper::of_stroke)
                })
                .unwrap_or_default(),
        )
    }

    /// The page counter: blank pages are counted as part of the content page before them.
    pub fn doc_page_label(&self) -> PageLabel {
        let n = self.doc_n_pages();
        let current = self.doc_current_page();
        let content = (0..n)
            .map(|page| self.page_is_content(page))
            .collect::<Vec<bool>>();
        if !content.iter().any(|&c| c) {
            // Without any document content every page counts.
            return PageLabel {
                page: current + 1,
                n_pages: n,
                ..Default::default()
            };
        }
        // The content page that the current page belongs to.
        let Some(owner) = (0..=current as usize).rev().find(|&i| content[i]) else {
            // Blank pages before the first content page.
            return PageLabel {
                page: 0,
                n_pages: content.iter().filter(|&&c| c).count() as u32,
                n_blank: content.iter().take_while(|&&c| !c).count() as u32,
                blank: current + 1,
            };
        };
        let n_blank = content[owner + 1..].iter().take_while(|&&c| !c).count() as u32;
        PageLabel {
            page: content[..=owner].iter().filter(|&&c| c).count().max(1) as u32,
            n_pages: content.iter().filter(|&&c| c).count() as u32,
            n_blank,
            blank: current - owner as u32,
        }
    }

    /// Fit the document size to its content after undo or redo, since the history only
    /// covers the strokes and page inserts and deletes change the document height.
    pub(crate) fn doc_resize_paged(&mut self) -> WidgetFlags {
        if !self.doc_single_page() {
            return WidgetFlags::default();
        }
        let Some(format_height) = self.format_height() else {
            return WidgetFlags::default();
        };
        // The pages are the PDF pages and the paper of blank pages. Ink that reaches past them
        // does not add pages. Without such strokes the pages are left as they are.
        let Some(bottom) = self
            .store
            .stroke_keys_unordered()
            .into_iter()
            .filter(|&key| self.store.stroke_layer(key) == Some(StrokeLayer::Document))
            .filter_map(|key| self.store.get_stroke_ref(key).map(|s| s.bounds().maxs.y))
            .reduce(f64::max)
        else {
            return WidgetFlags::default();
        };
        let mut widget_flags = WidgetFlags::default();
        let n_pages = ((bottom - self.document.y) / format_height)
            .round()
            .max(1.0);
        let height = n_pages * format_height;
        if (self.document.height - height).abs() > Self::PAGE_BORDER_TOLERANCE {
            self.document.height = height;
            widget_flags.resize = true;
            widget_flags |= self.update_rendering_current_viewport();
        }
        if self.document.shown_page >= self.doc_n_pages() {
            self.document.needs_fit = true;
        }
        widget_flags
    }

    /// Show the page with the given index, fitted into the viewport.
    pub fn doc_go_to_page(&mut self, page: u32) -> WidgetFlags {
        let n_pages = self.doc_n_pages();
        if n_pages == 0 {
            return WidgetFlags::default();
        }
        self.document.shown_page = page.min(n_pages - 1);
        self.doc_fit_current_page()
    }

    /// Zoom so that the current page fits into the viewport, and center it.
    pub fn doc_fit_current_page(&mut self) -> WidgetFlags {
        let mut widget_flags = WidgetFlags::default();
        let page = self.doc_current_page();
        let format_size = self.document.config.format.size();
        let viewport_size = self.camera.size() - Vector2::splat(2.0 * Camera::SINGLE_PAGE_MARGIN);
        let Some(center) = self.doc_page_center(page) else {
            return widget_flags;
        };
        if format_size.x <= 0.0 || viewport_size.x <= 0.0 || viewport_size.y <= 0.0 {
            return widget_flags;
        }
        let zoom = (viewport_size.x / format_size.x)
            .min(viewport_size.y / format_size.y)
            .clamp(Camera::ZOOM_MIN, Camera::ZOOM_MAX);
        self.document.needs_fit = false;
        // A pending zoom from a pinch or swipe gesture would undo the fit.
        self.camera.cancel_zoom_timeout();
        if (zoom - self.camera.total_zoom()).abs() > 1e-6 {
            // Zoom permanently right away, so the page is rendered sharply at once.
            widget_flags |= self.camera.zoom_temporarily_to(1.0) | self.camera.zoom_to(zoom);
            let all_strokes = self.store.stroke_keys_unordered();
            self.store.set_rendering_dirty_for_strokes(&all_strokes);
            widget_flags |= self.background_rendering_regenerate();
        }
        let offset = center * self.camera.total_zoom() - self.camera.size() * 0.5;
        widget_flags |= self.camera_set_offset(offset);
        widget_flags |= self.update_rendering_current_viewport();
        widget_flags.resize = true;
        widget_flags
    }

    /// The center of the page with the given index, in document coordinates.
    pub fn doc_page_center(&self, page: u32) -> Option<Vector2> {
        let format_height = self.format_height()?;
        Some(Vector2::new(
            self.document.x + self.document.width * 0.5,
            self.document.y + (page as f64 + 0.5) * format_height,
        ))
    }

    /// Whether there is a page `delta` pages away from the current page.
    pub fn doc_can_go_to_page_relative(&self, delta: i32) -> bool {
        let page = self.doc_current_page() as i64 + delta as i64;
        page >= 0 && page < self.doc_n_pages() as i64
    }

    /// Scroll `delta` pages forward (or backward when negative) from the current page.
    pub fn doc_go_to_page_relative(&mut self, delta: i32) -> WidgetFlags {
        let page = (self.doc_current_page() as i64 + delta as i64).max(0) as u32;
        self.doc_go_to_page(page)
    }

    fn insert_paper(&mut self, page: u32, paper: BlankPaper) {
        let Some(format_height) = self.format_height() else {
            return;
        };
        let size = Vector2::new(self.document.config.format.width(), format_height);
        let pos = Vector2::new(
            self.document.x,
            self.document.y + page as f64 * format_height,
        );
        match VectorImage::from_svg_str(&paper.svg(size), pos, ImageSizeOption::ImposeSize(size)) {
            Ok(image) => {
                let key = self
                    .store
                    .insert_stroke(Stroke::VectorImage(image), Some(StrokeLayer::Document));
                self.store.regenerate_rendering_for_stroke(
                    key,
                    self.camera.viewport(),
                    self.camera.image_scale(),
                );
            }
            Err(e) => tracing::error!("Creating blank page paper failed, Err: {e:?}"),
        }
    }

    /// Insert a blank page with the given paper after the current page when in fixed size layout,
    /// moving all following content down by one page, and scroll to it.
    pub fn doc_insert_page_after_current(&mut self, paper: BlankPaper) -> WidgetFlags {
        let mut widget_flags = WidgetFlags::default();
        let Some(format_height) = self.format_height() else {
            return widget_flags;
        };
        if self.document.config.layout != Layout::FixedSize {
            return widget_flags;
        }
        let page = self.doc_current_page();
        let split_y = self.document.y + (page + 1) as f64 * format_height;

        let below = self
            .store
            .keys_below_y(split_y - Self::PAGE_BORDER_TOLERANCE);
        let offset = Vector2::new(0.0, format_height);
        self.store.translate_strokes(&below, offset);
        self.store.translate_strokes_images(&below, offset);
        self.document.add_page_fixed_size();
        self.insert_paper(page + 1, paper);

        widget_flags |= self.record(Instant::now());
        widget_flags.resize = true;
        widget_flags.store_modified = true;
        widget_flags | self.doc_go_to_page(page + 1)
    }

    /// Change the paper of the current page, when it is a blank page.
    pub fn doc_set_current_page_paper(&mut self, paper: BlankPaper) -> WidgetFlags {
        let mut widget_flags = WidgetFlags::default();
        if !self.doc_current_page_is_blank() || self.doc_current_page_paper() == Some(paper) {
            return widget_flags;
        }
        let page = self.doc_current_page();
        let old_paper = self
            .page_keys(page)
            .into_iter()
            .filter(|&key| {
                self.store
                    .get_stroke_ref(key)
                    .and_then(BlankPaper::of_stroke)
                    .is_some()
            })
            .collect::<Vec<StrokeKey>>();
        self.store.set_trashed_keys(&old_paper, true);
        self.insert_paper(page, paper);

        widget_flags |= self.record(Instant::now()) | self.update_rendering_current_viewport();
        widget_flags.store_modified = true;
        widget_flags
    }

    /// Delete the current page with everything on it when it is a blank page and in fixed size
    /// layout, moving all following content up by one page.
    ///
    /// Returns whether the page was deleted.
    pub fn doc_delete_current_page_if_blank(&mut self) -> (bool, WidgetFlags) {
        let mut widget_flags = WidgetFlags::default();
        let Some(format_height) = self.format_height() else {
            return (false, widget_flags);
        };
        if self.document.config.layout != Layout::FixedSize
            || self.doc_n_pages() <= 1
            || !self.doc_current_page_is_blank()
        {
            return (false, widget_flags);
        }
        let page = self.doc_current_page();
        // Strokes that start on the page above and only reach into this page are kept.
        let on_page = self.page_own_keys(page);
        self.store.set_trashed_keys(&on_page, true);

        let page_bottom = self.document.y + (page + 1) as f64 * format_height;
        let below = self
            .store
            .keys_below_y(page_bottom - Self::PAGE_BORDER_TOLERANCE);
        let offset = Vector2::new(0.0, -format_height);
        self.store.translate_strokes(&below, offset);
        self.store.translate_strokes_images(&below, offset);
        self.document.remove_page_fixed_size();

        widget_flags |= self.record(Instant::now());
        widget_flags.resize = true;
        widget_flags.store_modified = true;
        (true, widget_flags | self.doc_go_to_page(page))
    }

    /// Whether the current page has anything that [Self::doc_clear_current_page] would remove.
    pub fn doc_current_page_has_ink(&self) -> bool {
        !self.page_ink_keys(self.doc_current_page()).is_empty()
    }

    /// Everything written on a page: all strokes except the document content and paper.
    fn page_ink_keys(&self, page: u32) -> Vec<StrokeKey> {
        self.page_own_keys(page)
            .into_iter()
            .filter(|&key| self.store.stroke_layer(key) != Some(StrokeLayer::Document))
            .collect()
    }

    /// Remove everything written on the current page (undoable).
    pub fn doc_clear_current_page(&mut self) -> WidgetFlags {
        let mut widget_flags = WidgetFlags::default();
        let keys = self.page_ink_keys(self.doc_current_page());
        if keys.is_empty() {
            return widget_flags;
        }
        self.store.set_trashed_keys(&keys, true);
        widget_flags |= self.record(Instant::now()) | self.update_rendering_current_viewport();
        widget_flags.store_modified = true;
        widget_flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper_stroke(svg: &str) -> Stroke {
        let size = Vector2::new(320.0, 180.0);
        Stroke::VectorImage(
            VectorImage::from_svg_str(svg, Vector2::ZERO, ImageSizeOption::ImposeSize(size))
                .unwrap(),
        )
    }

    #[test]
    fn paper_is_recognized() {
        let size = Vector2::new(320.0, 180.0);
        for paper in [BlankPaper::Plain, BlankPaper::Lined, BlankPaper::Grid] {
            let stroke = paper_stroke(&paper.svg(size));
            assert_eq!(BlankPaper::of_stroke(&stroke), Some(paper));
        }
    }

    #[test]
    fn page_with_paper_color_and_more_is_not_paper() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180"><rect x="0" y="0" width="320" height="180" fill="#fffffe"/><rect x="10" y="10" width="50" height="50" fill="#123456"/></svg>"##;
        assert_eq!(BlankPaper::of_stroke(&paper_stroke(svg)), None);
    }

    /// Draw a stroke from `from` to `to` with the current pen.
    fn draw_stroke(engine: &mut Engine, from: Vector2, to: Vector2, now: &mut Instant) {
        use rnote_compose::penevent::PenEvent;
        use rnote_compose::penpath::Element;

        for i in 0..=10 {
            *now += std::time::Duration::from_millis(8);
            let pos = from + (to - from) * (i as f64 / 10.0);
            let _ = engine.handle_pen_event(
                PenEvent::Down {
                    element: Element::new(pos, 0.5),
                    modifier_keys: Default::default(),
                },
                None,
                *now,
            );
        }
        let _ = engine.handle_pen_event(
            PenEvent::Up {
                element: Element::new(to, 0.5),
                modifier_keys: Default::default(),
            },
            None,
            *now,
        );
    }

    /// Drawing on the second page of an A4 document after undo and redo panicked in rstar, when
    /// the rebuilt key tree was unbalanced.
    #[test]
    fn draw_on_second_page_after_undo_redo() {
        for undo_interval in [3, 5, 7] {
            let mut engine = Engine::default();
            engine.document.config.layout = Layout::FixedSize;
            let format_height = engine.document.config.format.height();
            engine.document.height = 3.0 * format_height;
            for page in 0..3 {
                engine.insert_paper(page, BlankPaper::Plain);
            }
            let _ = engine.camera_set_size(Vector2::new(1200.0, 800.0));
            let _ = engine.doc_go_to_page(1);
            let _ = engine.change_pen_style(crate::pens::PenStyle::Brush);
            let page = engine.document.single_page_bounds().unwrap();

            let mut now = Instant::now();
            for i in 0..150 {
                // Strokes scattered over the page.
                let from = page.mins
                    + Vector2::new((i * 37 % 520) as f64 + 10.0, (i * 91 % 780) as f64 + 10.0);
                draw_stroke(&mut engine, from, from + Vector2::new(30.0, 15.0), &mut now);
                // Undo and redo rebuild the key tree, and the next strokes are inserted into it.
                if i % undo_interval == 0 {
                    let _ = engine.undo(now);
                    assert!(engine.store.key_tree_is_balanced());
                    let _ = engine.redo(now);
                    assert!(engine.store.key_tree_is_balanced());
                }
            }
            assert_eq!(engine.store.stroke_keys_unordered().len(), 3 + 150);
        }
    }
}
