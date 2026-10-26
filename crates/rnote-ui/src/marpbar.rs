//! The annotation toolbar, modelled after the MarpX notes toolbar: one dark bar docked to an edge
//! of the window, with large previous / next page buttons at its ends and pill shaped groups for
//! the tools, colors, sizes, history, pages and files in between.

// Imports
use crate::RnAppWindow;
use gtk4::{
    Align, Button, CssProvider, FlowBox, Grid, Image, Label, Orientation, PolicyType,
    ScrolledWindow, Widget, gdk, glib, glib::clone, prelude::*, subclass::prelude::*,
};
use rnote_compose::Color;
use rnote_engine::document::Layout;
use rnote_engine::engine::{BlankPaper, PageLabel};
use rnote_engine::pens::PenStyle;
use rnote_engine::pens::pensconfig::PensConfig;
use rnote_engine::pens::pensconfig::brushconfig::BrushStyle;
use rnote_engine::pens::pensconfig::eraserconfig::EraserStyle;
use rnote_engine::pens::pensconfig::selectorconfig::SelectorStyle;
use rnote_engine::pens::pensconfig::toolsconfig::ToolStyle;
use std::cell::{Cell, RefCell};

/// The tools of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarpTool {
    Pen,
    Highlighter,
    Selector,
    Laser,
    Eraser,
    Partial,
}

impl MarpTool {
    const ALL: [Self; 6] = [
        Self::Pen,
        Self::Highlighter,
        Self::Selector,
        Self::Laser,
        Self::Eraser,
        Self::Partial,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Pen => "Pen",
            Self::Highlighter => "Highlighter",
            Self::Selector => "Select (drag: box, circle: lasso)",
            Self::Laser => "Laser pointer",
            Self::Eraser => "Stroke eraser: removes whole strokes",
            Self::Partial => "Partial eraser: erases only what it touches",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Pen => ICON_PEN,
            Self::Highlighter => ICON_HIGHLIGHTER,
            Self::Selector => ICON_SELECTOR,
            Self::Laser => ICON_LASER,
            Self::Eraser => ICON_ERASER,
            Self::Partial => ICON_PARTIAL,
        }
    }

    fn caption(self) -> Option<&'static str> {
        match self {
            Self::Eraser => Some("stroke"),
            Self::Partial => Some("part"),
            _ => None,
        }
    }

    fn has_color(self) -> bool {
        matches!(self, Self::Pen | Self::Highlighter)
    }

    /// The four sizes on a page that is [REFERENCE_WIDTH] wide: the stroke width for the pen and
    /// highlighter, the radius for the erasers.
    fn sizes(self) -> Option<[f64; 4]> {
        match self {
            Self::Pen => Some([2.0, 4.0, 7.0, 12.0]),
            Self::Highlighter => Some([12.0, 20.0, 32.0, 48.0]),
            Self::Eraser | Self::Partial => Some([8.0, 16.0, 32.0, 64.0]),
            Self::Selector | Self::Laser => None,
        }
    }

    fn pen_style(self) -> PenStyle {
        match self {
            Self::Pen | Self::Highlighter => PenStyle::Brush,
            Self::Selector => PenStyle::Selector,
            Self::Laser => PenStyle::Tools,
            Self::Eraser | Self::Partial => PenStyle::Eraser,
        }
    }

    /// The tool for the current pen style, if it is one of the bar.
    fn from_pen_style(pen_style: PenStyle, c: &PensConfig) -> Option<Self> {
        match pen_style {
            PenStyle::Brush if c.brush_config.style == BrushStyle::Marker => {
                Some(Self::Highlighter)
            }
            PenStyle::Brush => Some(Self::Pen),
            PenStyle::Selector => Some(Self::Selector),
            PenStyle::Tools if c.tools_config.style == ToolStyle::Laser => Some(Self::Laser),
            PenStyle::Eraser => match c.eraser_config.style {
                EraserStyle::TrashCollidingStrokes => Some(Self::Eraser),
                EraserStyle::SplitCollidingStrokes => Some(Self::Partial),
            },
            _ => None,
        }
    }

    /// Select the sub-style of this tool in the config.
    fn apply_style(self, c: &mut PensConfig) {
        match self {
            Self::Pen => c.brush_config.style = BrushStyle::Solid,
            Self::Highlighter => c.brush_config.style = BrushStyle::Marker,
            Self::Selector => c.selector_config.style = SelectorStyle::Polygon,
            Self::Laser => c.tools_config.style = ToolStyle::Laser,
            Self::Eraser => c.eraser_config.style = EraserStyle::TrashCollidingStrokes,
            Self::Partial => c.eraser_config.style = EraserStyle::SplitCollidingStrokes,
        }
    }

    /// The size in the config, in document units.
    fn size(self, c: &PensConfig) -> Option<f64> {
        match self {
            Self::Pen => Some(c.brush_config.solid_options.stroke_width),
            Self::Highlighter => Some(c.brush_config.marker_options.stroke_width),
            Self::Eraser | Self::Partial => Some(c.eraser_config.width * 0.5),
            Self::Selector | Self::Laser => None,
        }
    }

    fn set_size(self, c: &mut PensConfig, size: f64) {
        match self {
            Self::Pen => c.brush_config.solid_options.stroke_width = size,
            Self::Highlighter => c.brush_config.marker_options.stroke_width = size,
            Self::Eraser | Self::Partial => c.eraser_config.width = size * 2.0,
            Self::Selector | Self::Laser => {}
        }
    }

    fn color(self, c: &PensConfig) -> Option<Color> {
        match self {
            Self::Pen => c.brush_config.solid_options.stroke_color,
            Self::Highlighter => c.brush_config.marker_options.stroke_color,
            _ => None,
        }
    }

    fn set_color(self, c: &mut PensConfig, color: Color) {
        match self {
            Self::Pen => {
                c.brush_config.solid_options.stroke_color = Some(color);
                c.brush_config.textured_options.stroke_color = Some(color);
            }
            Self::Highlighter => c.brush_config.marker_options.stroke_color = Some(color),
            _ => {}
        }
    }
}

/// Tool sizes are given for a page this wide, and scaled to the width of the document format.
const REFERENCE_WIDTH: f64 = 1280.0;

const COLORS: [(&str, &str); 8] = [
    ("Black", "#000000"),
    ("White", "#ffffff"),
    ("Red", "#be1e3c"),
    ("Blue", "#1e64c8"),
    ("Green", "#2e8b3a"),
    ("Orange", "#f08c00"),
    ("Yellow", "#ffd200"),
    ("Purple", "#7a3c96"),
];

const SIZE_LABELS: [&str; 4] = ["S", "M", "L", "XL"];
const SIZE_DOTS: [u32; 4] = [4, 8, 13, 19];

const PAPERS: [(BlankPaper, &str, &str); 3] = [
    (BlankPaper::Plain, "Plain paper", ICON_PAGE_PLAIN),
    (BlankPaper::Lined, "Lined paper", ICON_PAGE_LINED),
    (BlankPaper::Grid, "Grid paper", ICON_PAGE_GRID),
];

fn parse_hex(hex: &str) -> Color {
    let v = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    Color::new(v(1), v(3), v(5), 1.0)
}

// ── Icons (24×24, stroked) ─────────────────────────────────────────

const ICON_PEN: &str =
    r#"<path d="M3 21l3.5-1L19 7.5 16.5 5 4 17.5z"/><path d="M14.5 7l2.5 2.5"/>"#;
const ICON_HIGHLIGHTER: &str = r#"<path d="M9 14l-3 3v3h3l3-3"/><path d="M9 14l7-9 4 4-9 7z"/><path d="M3 21h6" stroke-width="3"/>"#;
const ICON_SELECTOR: &str = r#"<rect x="3" y="3" width="13" height="13" stroke-dasharray="3 2.5"/><path d="M12 12l9 3.5-4 1.5-1.5 4z" fill="currentColor"/>"#;
const ICON_LASER: &str = r#"<circle cx="12" cy="12" r="3.2" fill="currentColor"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/>"#;
const ICON_ERASER: &str = r#"<path d="M6 11l6-6 5 5-6 6H8z"/><path d="M9 8l5 5"/><path d="M2.5 20c3-2.5 5-2.5 7 0s5 2.5 7 0 3.5-1.5 5-1"/>"#;
const ICON_PARTIAL: &str =
    r#"<path d="M9 13l5-5 4 4-5 5z" fill="currentColor"/><path d="M2 20h7M16 20h6"/>"#;
const ICON_UNDO: &str = r#"<path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/>"#;
const ICON_REDO: &str = r#"<path d="M15 14l5-5-5-5"/><path d="M20 9H10a6 6 0 000 12h3"/>"#;
const ICON_CLEAR: &str =
    r#"<path d="M4 7h16M10 11v6M14 11v6"/><path d="M6 7l1 13h10l1-13M9 7V4h6v3"/>"#;
const ICON_PREV: &str = r#"<path d="M15 5l-7 7 7 7"/>"#;
const ICON_NEXT: &str = r#"<path d="M9 5l7 7-7 7"/>"#;
const ICON_PAGE_INSERT: &str =
    r#"<path d="M6 3h9l4 4v14H6z"/><path d="M15 3v4h4"/><path d="M12.5 10.5v7M9 14h7"/>"#;
const ICON_PAGE_PLAIN: &str = r#"<path d="M6 3h9l4 4v14H6z"/><path d="M15 3v4h4"/>"#;
const ICON_PAGE_LINED: &str = r#"<path d="M6 3h9l4 4v14H6z"/><path d="M9 11h7M9 14h7M9 17h7"/>"#;
const ICON_PAGE_GRID: &str =
    r#"<path d="M6 3h9l4 4v14H6z"/><path d="M10 9v12M14 9v12M6 13h13M6 17h13"/>"#;
const ICON_PAGE_DELETE: &str =
    r#"<path d="M6 3h9l4 4v14H6z"/><path d="M9.5 12.5l5 5M14.5 12.5l-5 5"/>"#;
const ICON_SAVE: &str = r#"<path d="M12 3v12M7 10l5 5 5-5"/><path d="M4 17v3h16v-3"/>"#;
const ICON_LOAD: &str = r#"<path d="M12 15V3M7 8l5-5 5 5"/><path d="M4 17v3h16v-3"/>"#;
const ICON_PDF: &str = r#"<path d="M7 9V3h10v6"/><rect x="3" y="9" width="18" height="8" rx="1.5"/><path d="M7 14h10v7H7z"/>"#;
const ICON_DOCK_TOP: &str =
    r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18" stroke-width="3.2"/>"#;
const ICON_DOCK_RIGHT: &str =
    r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M16 4v16" stroke-width="3.2"/>"#;
const ICON_DOCK_BOTTOM: &str =
    r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 15h18" stroke-width="3.2"/>"#;
const ICON_DOCK_LEFT: &str =
    r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M8 4v16" stroke-width="3.2"/>"#;

const FG: &str = "#f2f2f2";

/// Rendered at three times the shown size, so the icons stay sharp on scaled displays.
fn icon_texture(paths: &str, size: i32) -> Option<gdk::Texture> {
    let px = size * 3;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 24 24" fill="none" stroke="{FG}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
        paths.replace("currentColor", FG)
    );
    gdk::Texture::from_bytes(&glib::Bytes::from_owned(svg.into_bytes()))
        .inspect_err(|e| tracing::error!("Loading toolbar icon failed, Err: {e:?}"))
        .ok()
}

/// Activate an action of the app window, as the corresponding menu entry or shortcut would.
fn activate_win_action(appwindow: &RnAppWindow, name: &str) {
    gtk4::gio::prelude::ActionGroupExt::activate_action(appwindow, name, None);
}

fn icon_image(paths: &str, size: i32) -> Image {
    let image = Image::new();
    if let Some(texture) = icon_texture(paths, size) {
        image.set_paintable(Some(&texture));
    }
    image.set_pixel_size(size);
    image
}

// ── Dock ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Dock {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
}

impl Dock {
    fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            _ => Self::Top,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Top => Self::Right,
            Self::Right => Self::Bottom,
            Self::Bottom => Self::Left,
            Self::Left => Self::Top,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Top => ICON_DOCK_TOP,
            Self::Right => ICON_DOCK_RIGHT,
            Self::Bottom => ICON_DOCK_BOTTOM,
            Self::Left => ICON_DOCK_LEFT,
        }
    }

    fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

// ── Items ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum ItemKind {
    Dock,
    Tool(MarpTool),
    Color(usize),
    Size(usize),
    Undo,
    Redo,
    Clear,
    PagePrev,
    PageNext,
    PageInsert,
    Paper(BlankPaper),
    PageDelete,
    Save,
    Open,
    Pdf,
}

#[derive(Debug)]
struct Item {
    kind: ItemKind,
    button: Button,
}

const CSS: &str = r#"
.marp-bar {
  background-color: rgba(28, 28, 30, 0.95);
  color: #f2f2f2;
  padding: 4px 6px;
  box-shadow: 0 0 6px rgba(0, 0, 0, 0.35);
  font-size: 13px;
}
.marp-bar.vertical { padding: 6px 4px; }
.marp-bar flowbox, .marp-bar flowboxchild, .marp-bar scrolledwindow, .marp-bar viewport {
  padding: 0; margin: 0; background: none; border: none; min-height: 0; min-width: 0;
}
.marp-group {
  padding: 2px;
  border-radius: 10px;
  background-color: rgba(255, 255, 255, 0.07);
}
button.marp-btn {
  min-width: 36px;
  min-height: 36px;
  padding: 0;
  margin: 0;
  border: none;
  border-radius: 8px;
  background: none;
  box-shadow: none;
  color: #f2f2f2;
  outline-color: #8ab4ff;
}
button.marp-btn:hover { background-color: rgba(255, 255, 255, 0.12); }
button.marp-btn:active { background-color: rgba(255, 255, 255, 0.18); }
button.marp-btn:disabled { opacity: 0.3; background: none; }
button.marp-btn.is-active { background-color: #be1e3c; color: #ffffff; }
.marp-edge button.marp-btn { min-width: 56px; min-height: 48px; }
.marp-cap { font-size: 9px; color: #f2f2f2; }
.marp-swatch-dot {
  min-width: 22px;
  min-height: 22px;
  border-radius: 11px;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.45);
}
button.marp-btn.marp-swatch.is-active { background: none; box-shadow: inset 0 0 0 2px #ffffff; }
.marp-size-dot { background-color: #f2f2f2; }
.marp-label { padding: 0 4px; font-size: 12px; font-feature-settings: "tnum"; color: #f2f2f2; opacity: 0.9; }
"#;

fn bar_css() -> String {
    let mut css = CSS.to_string();
    for (i, (_, hex)) in COLORS.iter().enumerate() {
        css.push_str(&format!(
            ".marp-swatch-{i} {{ background-color: {hex}; }}\n"
        ));
    }
    for (i, px) in SIZE_DOTS.iter().enumerate() {
        css.push_str(&format!(
            ".marp-size-{i} {{ min-width: {px}px; min-height: {px}px; border-radius: {}px; }}\n",
            px.div_ceil(2)
        ));
    }
    css
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct RnMarpBar {
        pub(super) dock: Cell<Dock>,
        pub(super) paper: Cell<BlankPaper>,
        pub(super) refresh_queued: Cell<bool>,
        pub(super) items: RefCell<Vec<Item>>,
        pub(super) groups: RefCell<Vec<Grid>>,
        pub(super) dock_image: RefCell<Option<Image>>,
        pub(super) page_label: Label,
        pub(super) edge_start: Grid,
        pub(super) edge_end: Grid,
        pub(super) flowbox: FlowBox,
        pub(super) scroller: ScrolledWindow,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RnMarpBar {
        const NAME: &'static str = "RnMarpBar";
        type Type = super::RnMarpBar;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for RnMarpBar {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("marp-bar");
            obj.set_spacing(4);

            for edge in [&self.edge_start, &self.edge_end] {
                edge.add_css_class("marp-group");
                edge.add_css_class("marp-edge");
                edge.set_valign(Align::Center);
                edge.set_halign(Align::Center);
            }

            self.flowbox.set_selection_mode(gtk4::SelectionMode::None);
            self.flowbox.set_homogeneous(false);
            self.flowbox.set_column_spacing(4);
            self.flowbox.set_row_spacing(4);
            self.flowbox.set_halign(Align::Center);
            self.flowbox.set_valign(Align::Center);
            self.scroller.set_child(Some(&self.flowbox));

            obj.append(&self.edge_start);
            obj.append(&self.scroller);
            obj.append(&self.edge_end);
        }
    }

    impl WidgetImpl for RnMarpBar {}
    impl BoxImpl for RnMarpBar {}
}

glib::wrapper! {
    pub(crate) struct RnMarpBar(ObjectSubclass<imp::RnMarpBar>)
        @extends gtk4::Box, Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Default for RnMarpBar {
    fn default() -> Self {
        Self::new()
    }
}

impl RnMarpBar {
    const DOCK_SETTINGS_KEY: &'static str = "marpbar-dock";

    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    /// The paper for new blank pages.
    pub(crate) fn paper(&self) -> BlankPaper {
        self.imp().paper.get()
    }

    pub(crate) fn init(&self, appwindow: &RnAppWindow) {
        let imp = self.imp();

        // The style is shared by the bars of all windows on the display.
        thread_local! {
            static CSS_INSTALLED: Cell<bool> = const { Cell::new(false) };
        }
        if !CSS_INSTALLED.replace(true) {
            let provider = CssProvider::new();
            provider.load_from_string(&bar_css());
            gtk4::style_context_add_provider_for_display(
                &self.display(),
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }

        // Dock position
        let dock = self.add_item(appwindow, None, ItemKind::Dock, "Move toolbar", None);
        let dock_image = icon_image(ICON_DOCK_TOP, 22);
        dock.set_child(Some(&dock_image));
        imp.dock_image.replace(Some(dock_image));
        self.end_group();

        // Tools
        for tool in MarpTool::ALL {
            self.add_item(
                appwindow,
                Some(tool.icon()),
                ItemKind::Tool(tool),
                tool.label(),
                tool.caption(),
            );
        }
        self.end_group();

        // Colors
        for (i, (label, _)) in COLORS.iter().enumerate() {
            let button = self.add_item(appwindow, None, ItemKind::Color(i), label, None);
            button.add_css_class("marp-swatch");
            let dot = gtk4::Box::new(Orientation::Horizontal, 0);
            dot.add_css_class("marp-swatch-dot");
            dot.add_css_class(&format!("marp-swatch-{i}"));
            dot.set_halign(Align::Center);
            dot.set_valign(Align::Center);
            button.set_child(Some(&dot));
        }
        self.end_group();

        // Sizes
        for (i, label) in SIZE_LABELS.iter().enumerate() {
            let button = self.add_item(
                appwindow,
                None,
                ItemKind::Size(i),
                &format!("Size {label}"),
                None,
            );
            let dot = gtk4::Box::new(Orientation::Horizontal, 0);
            dot.add_css_class("marp-size-dot");
            dot.add_css_class(&format!("marp-size-{i}"));
            dot.set_halign(Align::Center);
            dot.set_valign(Align::Center);
            button.set_child(Some(&dot));
        }
        self.end_group();

        // History
        self.add_item(
            appwindow,
            Some(ICON_UNDO),
            ItemKind::Undo,
            "Undo (Ctrl+Z)",
            None,
        );
        self.add_item(
            appwindow,
            Some(ICON_REDO),
            ItemKind::Redo,
            "Redo (Ctrl+Shift+Z)",
            None,
        );
        self.add_item(
            appwindow,
            Some(ICON_CLEAR),
            ItemKind::Clear,
            "Clear page (undoable)",
            None,
        );
        self.end_group();

        // Pages: previous and next sit large at the two ends of the bar.
        let prev = self.add_item(
            appwindow,
            Some(ICON_PREV),
            ItemKind::PagePrev,
            "Previous page (Alt+Page Up)",
            None,
        );
        self.take_last_into(&imp.edge_start, &prev);
        imp.page_label.add_css_class("marp-label");
        imp.page_label.set_width_chars(10);
        imp.page_label.set_tooltip_text(Some("Page"));
        self.current_group().attach(&imp.page_label, 0, 0, 1, 1);
        let next = self.add_item(
            appwindow,
            Some(ICON_NEXT),
            ItemKind::PageNext,
            "Next page (Alt+Page Down)",
            None,
        );
        self.take_last_into(&imp.edge_end, &next);
        self.add_item(
            appwindow,
            Some(ICON_PAGE_INSERT),
            ItemKind::PageInsert,
            "Insert blank page after this page (Ctrl+Shift+B)",
            None,
        );
        for (paper, label, icon) in PAPERS {
            self.add_item(
                appwindow,
                Some(icon),
                ItemKind::Paper(paper),
                &format!("{label} (for this and new blank pages)"),
                None,
            );
        }
        self.add_item(
            appwindow,
            Some(ICON_PAGE_DELETE),
            ItemKind::PageDelete,
            "Delete this blank page",
            None,
        );
        self.end_group();

        // File
        self.add_item(
            appwindow,
            Some(ICON_SAVE),
            ItemKind::Save,
            "Save notes (Ctrl+S)",
            None,
        );
        self.add_item(
            appwindow,
            Some(ICON_LOAD),
            ItemKind::Open,
            "Open (Ctrl+O)",
            None,
        );
        self.add_item(appwindow, Some(ICON_PDF), ItemKind::Pdf, "Export PDF", None);
        self.end_group();

        let dock = appwindow
            .app()
            .app_settings()
            .map(|s| Dock::from_name(&s.string(Self::DOCK_SETTINGS_KEY)))
            .unwrap_or_default();
        self.set_dock(appwindow, dock);
    }

    /// Refresh the bar once the pending events are handled, so that a burst of scroll or
    /// zoom events only refreshes it once.
    pub(crate) fn queue_refresh(&self, appwindow: &RnAppWindow) {
        if self.imp().refresh_queued.replace(true) {
            return;
        }
        glib::idle_add_local_once(clone!(
            #[weak(rename_to=marpbar)]
            self,
            #[weak]
            appwindow,
            move || {
                marpbar.imp().refresh_queued.set(false);
                marpbar.refresh(&appwindow);
            }
        ));
    }

    /// The group that items are currently added to.
    fn current_group(&self) -> Grid {
        let mut groups = self.imp().groups.borrow_mut();
        if groups.last().is_none_or(|g| g.parent().is_some()) {
            let group = Grid::new();
            group.add_css_class("marp-group");
            group.set_column_spacing(1);
            group.set_row_spacing(1);
            group.set_valign(Align::Center);
            group.set_halign(Align::Center);
            groups.push(group);
        }
        groups.last().unwrap().clone()
    }

    /// Finish the current group and add it to the bar.
    fn end_group(&self) {
        let group = self.current_group();
        self.imp().flowbox.append(&group);
    }

    /// Move a button that was just added to the current group into an edge group.
    fn take_last_into(&self, edge: &Grid, button: &Button) {
        self.current_group().remove(button);
        edge.attach(button, 0, 0, 1, 1);
    }

    fn add_item(
        &self,
        appwindow: &RnAppWindow,
        icon: Option<&str>,
        kind: ItemKind,
        tooltip: &str,
        caption: Option<&str>,
    ) -> Button {
        let button = Button::new();
        button.add_css_class("marp-btn");
        button.set_tooltip_text(Some(tooltip));
        button.set_focus_on_click(false);
        // The previous / next page buttons at the bar ends get larger icons.
        let edge = matches!(kind, ItemKind::PagePrev | ItemKind::PageNext);
        if let Some(icon) = icon {
            match caption {
                // An icon with a small caption underneath (the two erasers).
                Some(caption) => {
                    let content = gtk4::Box::new(Orientation::Vertical, 1);
                    content.set_valign(Align::Center);
                    content.append(&icon_image(icon, 18));
                    let label = Label::new(Some(caption));
                    label.add_css_class("marp-cap");
                    content.append(&label);
                    button.set_child(Some(&content));
                }
                None => {
                    let image = if edge {
                        icon_image(icon, 34)
                    } else {
                        icon_image(icon, 22)
                    };
                    button.set_child(Some(&image));
                }
            }
        }
        button.connect_clicked(clone!(
            #[weak(rename_to=marpbar)]
            self,
            #[weak]
            appwindow,
            move |_| marpbar.activate_item(&appwindow, kind)
        ));

        let group = self.current_group();
        let n = self.imp().items.borrow().len() as i32;
        group.attach(&button, n, 0, 1, 1);
        self.imp().items.borrow_mut().push(Item {
            kind,
            button: button.clone(),
        });
        button
    }

    /// Lay out the groups for the dock: one row when docked at the top or bottom, two columns
    /// when docked at the sides.
    fn relayout(&self) {
        let imp = self.imp();
        let vertical = imp.dock.get().is_vertical();
        for group in imp.groups.borrow().iter() {
            let mut children = Vec::new();
            let mut child = group.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                children.push(c);
            }
            // Children are kept in the order they were added.
            children.sort_by_key(|c| {
                let (column, row, _, _) = group.query_child(c);
                (row, column)
            });
            for c in &children {
                group.remove(c);
            }
            for (i, c) in children.iter().enumerate() {
                let i = i as i32;
                if vertical {
                    group.attach(c, i % 2, i / 2, 1, 1);
                } else {
                    group.attach(c, i, 0, 1, 1);
                }
            }
        }
    }

    fn set_dock(&self, appwindow: &RnAppWindow, dock: Dock) {
        let imp = self.imp();
        imp.dock.set(dock);
        if let Some(settings) = appwindow.app().app_settings()
            && let Err(e) = settings.set_string(Self::DOCK_SETTINGS_KEY, dock.name())
        {
            tracing::error!("Saving the toolbar dock failed, Err: {e:?}");
        }

        let vertical = dock.is_vertical();
        self.set_orientation(if vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        });
        if vertical {
            self.add_css_class("vertical");
            self.set_size_request(92, -1);
            imp.flowbox.set_max_children_per_line(1);
            imp.flowbox.set_min_children_per_line(1);
            imp.scroller
                .set_policy(PolicyType::Never, PolicyType::Automatic);
            imp.scroller.set_propagate_natural_height(false);
            imp.scroller.set_propagate_natural_width(true);
            imp.scroller.set_hexpand(false);
            imp.scroller.set_vexpand(true);
            imp.flowbox.set_valign(Align::Start);
        } else {
            self.remove_css_class("vertical");
            self.set_size_request(-1, -1);
            imp.flowbox.set_max_children_per_line(64);
            imp.flowbox.set_min_children_per_line(1);
            imp.scroller
                .set_policy(PolicyType::Never, PolicyType::Never);
            imp.scroller.set_propagate_natural_height(true);
            imp.scroller.set_propagate_natural_width(false);
            imp.scroller.set_hexpand(true);
            imp.scroller.set_vexpand(false);
            imp.flowbox.set_valign(Align::Center);
        }
        self.relayout();
        if let Some(image) = imp.dock_image.borrow().as_ref()
            && let Some(texture) = icon_texture(dock.icon(), 22)
        {
            image.set_paintable(Some(&texture));
        }
        for item in imp.items.borrow().iter() {
            if matches!(item.kind, ItemKind::Dock) {
                item.button.set_tooltip_text(Some(&format!(
                    "Move toolbar (now {}; next {})",
                    dock.name(),
                    dock.next().name()
                )));
            }
        }
        appwindow.overlays().place_marpbar(dock_to_position(dock));
    }

    fn activate_item(&self, appwindow: &RnAppWindow, kind: ItemKind) {
        if let ItemKind::Dock = kind {
            self.set_dock(appwindow, self.imp().dock.get().next());
            return;
        }
        let Some(canvas) = appwindow.active_tab_canvas() else {
            return;
        };
        match kind {
            ItemKind::Dock => {}
            ItemKind::Tool(tool) => {
                appwindow.set_pen_style_with_lock(tool.pen_style());
                // A locked pen keeps its style, and then its sub-style is kept too.
                if canvas.engine_ref().current_pen_style_w_override() != tool.pen_style() {
                    return;
                }
                tool.apply_style(&mut appwindow.engine_config().write().pens_config);
                let widget_flags = canvas.engine_mut().reinstall_pen_current_style();
                appwindow.handle_widget_flags(widget_flags, &canvas);
                appwindow.refresh_ui();
            }
            ItemKind::Color(i) => {
                if let Some(tool) = self.current_tool(appwindow) {
                    tool.set_color(
                        &mut appwindow.engine_config().write().pens_config,
                        parse_hex(COLORS[i].1),
                    );
                }
            }
            ItemKind::Size(i) => {
                if let Some(tool) = self.current_tool(appwindow)
                    && let Some(sizes) = tool.sizes()
                {
                    let scale = self.size_scale(appwindow);
                    tool.set_size(
                        &mut appwindow.engine_config().write().pens_config,
                        sizes[i] * scale,
                    );
                    let widget_flags = canvas.engine_mut().reinstall_pen_current_style();
                    appwindow.handle_widget_flags(widget_flags, &canvas);
                }
            }
            ItemKind::Undo => {
                activate_win_action(appwindow, "undo");
            }
            ItemKind::Redo => {
                activate_win_action(appwindow, "redo");
            }
            ItemKind::Clear => {
                let widget_flags = canvas.engine_mut().doc_clear_current_page();
                appwindow.handle_widget_flags(widget_flags, &canvas);
            }
            ItemKind::PagePrev => {
                activate_win_action(appwindow, "page-prev");
            }
            ItemKind::PageNext => {
                activate_win_action(appwindow, "page-next");
            }
            ItemKind::PageInsert => {
                activate_win_action(appwindow, "page-insert-after");
            }
            ItemKind::Paper(paper) => {
                self.imp().paper.set(paper);
                let widget_flags = canvas.engine_mut().doc_set_current_page_paper(paper);
                appwindow.handle_widget_flags(widget_flags, &canvas);
            }
            ItemKind::PageDelete => {
                activate_win_action(appwindow, "page-delete");
            }
            ItemKind::Save => {
                activate_win_action(appwindow, "save-doc");
            }
            ItemKind::Open => {
                activate_win_action(appwindow, "open-doc");
            }
            ItemKind::Pdf => {
                activate_win_action(appwindow, "export-doc");
            }
        }
        self.refresh(appwindow);
    }

    fn current_tool(&self, appwindow: &RnAppWindow) -> Option<MarpTool> {
        let canvas = appwindow.active_tab_canvas()?;
        let pen_style = canvas.engine_ref().current_pen_style_w_override();
        MarpTool::from_pen_style(pen_style, &appwindow.engine_config().read().pens_config)
    }

    /// Tool sizes scale with the width of the document format.
    fn size_scale(&self, appwindow: &RnAppWindow) -> f64 {
        appwindow
            .active_tab_canvas()
            .map(|canvas| canvas.engine_ref().document.config.format.width())
            .filter(|width| *width > 0.0)
            .map(|width| width / REFERENCE_WIDTH)
            .unwrap_or(1.0)
    }

    /// Update the active and sensitive state of all items and the page counter.
    pub(crate) fn refresh(&self, appwindow: &RnAppWindow) {
        let imp = self.imp();
        let Some(canvas) = appwindow.active_tab_canvas() else {
            return;
        };
        let tool = self.current_tool(appwindow);
        let pens_config = appwindow.engine_config().read().pens_config.clone();
        let scale = self.size_scale(appwindow);
        let size_index = tool.and_then(|tool| {
            let sizes = tool.sizes()?;
            let size = tool.size(&pens_config)? / scale;
            (0..sizes.len()).min_by(|&a, &b| {
                (sizes[a] - size)
                    .abs()
                    .partial_cmp(&(sizes[b] - size).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        let color = tool.and_then(|tool| tool.color(&pens_config));

        let engine = canvas.engine_ref();
        let fixed_size = engine.document.config.layout == Layout::FixedSize;
        let current_blank = engine.doc_current_page_is_blank();
        // On a blank page its paper is shown, elsewhere the paper for new blank pages.
        let current_paper = engine.doc_current_page_paper();
        let n_pages = engine.doc_n_pages();

        for item in imp.items.borrow().iter() {
            let (active, sensitive) = match item.kind {
                ItemKind::Dock => (false, true),
                ItemKind::Tool(t) => (tool == Some(t), true),
                ItemKind::Color(i) => (
                    color.is_some_and(|c| c.approx_eq(parse_hex(COLORS[i].1))),
                    // The highlighter multiplies its color, so white would not show.
                    tool.is_some_and(|t| {
                        t.has_color() && !(t == MarpTool::Highlighter && COLORS[i].1 == "#ffffff")
                    }),
                ),
                ItemKind::Size(i) => (
                    size_index == Some(i),
                    tool.is_some_and(|t| t.sizes().is_some()),
                ),
                ItemKind::Undo => (false, engine.can_undo()),
                ItemKind::Redo => (false, engine.can_redo()),
                ItemKind::Clear => (false, engine.doc_current_page_has_ink()),
                ItemKind::PagePrev => (false, engine.doc_can_go_to_page_relative(-1)),
                ItemKind::PageNext => (false, engine.doc_can_go_to_page_relative(1)),
                ItemKind::PageInsert => (false, fixed_size),
                ItemKind::Paper(paper) => (current_paper.unwrap_or(imp.paper.get()) == paper, true),
                ItemKind::PageDelete => (false, fixed_size && current_blank && n_pages > 1),
                ItemKind::Save | ItemKind::Open | ItemKind::Pdf => (false, true),
            };
            if active {
                item.button.add_css_class("is-active");
            } else {
                item.button.remove_css_class("is-active");
            }
            item.button.set_sensitive(sensitive);
        }

        let text = format_page_label(engine.doc_page_label());
        if imp.page_label.text() != text {
            imp.page_label.set_text(&text);
        }
    }
}

/// Compact page counter: "3 / 44" on a page, "3 / 44 · +2 blank" on a page followed by blank
/// pages, "3 / 44 · blank 1/2" on its first blank page.
fn format_page_label(label: PageLabel) -> String {
    let base = format!("{}\u{2009}/\u{2009}{}", label.page, label.n_pages);
    if label.n_blank == 0 {
        base
    } else if label.blank == 0 {
        format!("{base} · +{} blank", label.n_blank)
    } else {
        format!("{base} · blank {}/{}", label.blank, label.n_blank)
    }
}

/// Where the bar is placed relative to the canvas.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BarPosition {
    Start(Orientation),
    End(Orientation),
}

fn dock_to_position(dock: Dock) -> BarPosition {
    match dock {
        Dock::Top => BarPosition::Start(Orientation::Vertical),
        Dock::Left => BarPosition::Start(Orientation::Horizontal),
        Dock::Bottom => BarPosition::End(Orientation::Vertical),
        Dock::Right => BarPosition::End(Orientation::Horizontal),
    }
}
