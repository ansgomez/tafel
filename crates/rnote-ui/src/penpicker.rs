// Imports
use crate::RnAppWindow;
use gtk4::{
    Box, Button, CompositeTemplate, EventControllerLegacy, TemplateChild, ToggleButton, Widget,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use rnote_engine::pens::pensconfig::PensConfig;
use rnote_engine::pens::pensconfig::brushconfig::BrushStyle;
use rnote_engine::pens::pensconfig::eraserconfig::EraserStyle;
use rnote_engine::pens::pensconfig::toolsconfig::ToolStyle;
use rnote_engine::pens::{PenMode, PenStyle};

mod imp {
    use super::*;

    #[derive(Default, Debug, CompositeTemplate)]
    #[template(resource = "/com/github/flxzt/rnote/ui/penpicker.ui")]
    pub(crate) struct RnPenPicker {
        #[template_child]
        pub(crate) toolbox: TemplateChild<Box>,
        #[template_child]
        pub(crate) brush_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) highlighter_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) laser_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) eraser_split_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) shaper_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) typewriter_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) eraser_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) selector_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) tools_toggle: TemplateChild<ToggleButton>,
        #[template_child]
        pub(crate) undo_button: TemplateChild<Button>,
        #[template_child]
        pub(crate) redo_button: TemplateChild<Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RnPenPicker {
        const NAME: &'static str = "RnPenPicker";
        type Type = super::RnPenPicker;
        type ParentType = Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RnPenPicker {
        fn dispose(&self) {
            self.dispose_template();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for RnPenPicker {}
}

glib::wrapper! {
    pub(crate) struct RnPenPicker(ObjectSubclass<imp::RnPenPicker>)
        @extends Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RnPenPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl RnPenPicker {
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    pub(crate) fn undo_button(&self) -> Button {
        self.imp().undo_button.get()
    }

    pub(crate) fn redo_button(&self) -> Button {
        self.imp().redo_button.get()
    }

    pub(crate) fn init(&self, appwindow: &RnAppWindow) {
        let imp = self.imp();

        // controller to switch pen mode when hovering with a pen/eraser over the toolbox toolbar
        let pointer_controller_toolbox = EventControllerLegacy::builder()
            .name("pointer_controller_toolbar")
            .propagation_phase(gtk4::PropagationPhase::Bubble)
            .build();

        pointer_controller_toolbox.connect_event(clone!(
            #[weak]
            appwindow,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, event| {
                if let Some(canvas) = appwindow.active_tab_canvas()
                    && let Some(device) = event.device_tool()
                {
                    match device.tool_type() {
                        gtk4::gdk::DeviceToolType::Pen => {
                            let widget_flags = canvas.engine_mut().change_pen_mode(PenMode::Pen);
                            canvas.emit_handle_widget_flags(widget_flags);
                        }
                        gtk4::gdk::DeviceToolType::Eraser => {
                            let widget_flags = canvas.engine_mut().change_pen_mode(PenMode::Eraser);
                            canvas.emit_handle_widget_flags(widget_flags);
                        }
                        _ => {}
                    }
                }

                glib::Propagation::Proceed
            }
        ));

        imp.toolbox.add_controller(pointer_controller_toolbox);

        // One tap tools: each button selects a pen style together with its sub-style.
        // The closures return whether they changed the sub-style.
        Self::connect_quick_tool(&imp.brush_toggle, appwindow, PenStyle::Brush, |c| {
            // The pen is any brush style except the marker, which is the highlighter.
            let changed = c.brush_config.style == BrushStyle::Marker;
            if changed {
                c.brush_config.style = BrushStyle::Solid;
            }
            changed
        });
        Self::connect_quick_tool(&imp.highlighter_toggle, appwindow, PenStyle::Brush, |c| {
            let changed = c.brush_config.style != BrushStyle::Marker;
            c.brush_config.style = BrushStyle::Marker;
            changed
        });
        Self::connect_quick_tool(&imp.laser_toggle, appwindow, PenStyle::Tools, |c| {
            let changed = c.tools_config.style != ToolStyle::Laser;
            c.tools_config.style = ToolStyle::Laser;
            changed
        });
        Self::connect_quick_tool(&imp.tools_toggle, appwindow, PenStyle::Tools, |c| {
            // The laser has its own button, so this one selects the remaining tools.
            let changed = c.tools_config.style == ToolStyle::Laser;
            if changed {
                c.tools_config.style = ToolStyle::VerticalSpace;
            }
            changed
        });
        Self::connect_quick_tool(&imp.eraser_toggle, appwindow, PenStyle::Eraser, |c| {
            let changed = matches!(c.eraser_config.style, EraserStyle::SplitCollidingStrokes);
            c.eraser_config.style = EraserStyle::TrashCollidingStrokes;
            changed
        });
        Self::connect_quick_tool(&imp.eraser_split_toggle, appwindow, PenStyle::Eraser, |c| {
            let changed = matches!(c.eraser_config.style, EraserStyle::TrashCollidingStrokes);
            c.eraser_config.style = EraserStyle::SplitCollidingStrokes;
            changed
        });

        imp.shaper_toggle.connect_toggled(clone!(
            #[weak]
            appwindow,
            move |shaper_toggle| {
                if shaper_toggle.is_active() {
                    appwindow.set_pen_style_with_lock(PenStyle::Shaper);
                }
            }
        ));

        imp.typewriter_toggle.connect_toggled(clone!(
            #[weak]
            appwindow,
            move |typewriter_toggle| {
                if typewriter_toggle.is_active() {
                    appwindow.set_pen_style_with_lock(PenStyle::Typewriter);
                }
            }
        ));

        imp.selector_toggle.get().connect_toggled(clone!(
            #[weak]
            appwindow,
            move |selector_toggle| {
                if selector_toggle.is_active() {
                    appwindow.set_pen_style_with_lock(PenStyle::Selector);
                }
            }
        ));
    }

    /// Selects `pen_style` and applies `set_sub_style` to the pens config when `toggle` gets activated.
    fn connect_quick_tool(
        toggle: &ToggleButton,
        appwindow: &RnAppWindow,
        pen_style: PenStyle,
        set_sub_style: impl Fn(&mut PensConfig) -> bool + 'static,
    ) {
        toggle.connect_toggled(clone!(
            #[weak]
            appwindow,
            move |toggle| {
                if !toggle.is_active() {
                    return;
                }
                let changed = set_sub_style(&mut appwindow.engine_config().write().pens_config);
                appwindow.set_pen_style_with_lock(pen_style);
                if changed {
                    if let Some(canvas) = appwindow.active_tab_canvas() {
                        let widget_flags = canvas.engine_mut().reinstall_pen_current_style();
                        canvas.emit_handle_widget_flags(widget_flags);
                    }
                    // Sidebar pages, colors and widths follow the newly selected sub-style.
                    appwindow.refresh_ui();
                }
            }
        ));
    }

    /// Activates the button matching the current pen style and sub-style.
    pub(crate) fn sync_active_tool(&self, pen_style: PenStyle, pens_config: &PensConfig) {
        let imp = self.imp();
        let toggle = match pen_style {
            PenStyle::Brush if pens_config.brush_config.style == BrushStyle::Marker => {
                &imp.highlighter_toggle
            }
            PenStyle::Brush => &imp.brush_toggle,
            PenStyle::Shaper => &imp.shaper_toggle,
            PenStyle::Typewriter => &imp.typewriter_toggle,
            PenStyle::Eraser => match pens_config.eraser_config.style {
                EraserStyle::TrashCollidingStrokes => &imp.eraser_toggle,
                EraserStyle::SplitCollidingStrokes => &imp.eraser_split_toggle,
            },
            PenStyle::Selector => &imp.selector_toggle,
            PenStyle::Tools if pens_config.tools_config.style == ToolStyle::Laser => {
                &imp.laser_toggle
            }
            PenStyle::Tools => &imp.tools_toggle,
        };
        toggle.set_active(true);
    }
}
