use core::f32;

use egui::{
    CentralPanel, Color32, Id, Key, Response, RichText, ScrollArea, SidePanel, TextEdit,
    TopBottomPanel, Ui,
};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};
//use egui_extras::syntax_highlighting::{highlight, CodeTheme};

use crate::Runtime;

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct Project {
    code: String,
    run_mode: RunMode,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
pub struct TemplateApp {
    project: Project,
    runtime: crate::Runtime,
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let project: Project = cc.storage.and_then(|storage| eframe::get_value(storage, eframe::APP_KEY)).unwrap_or_default();

        let mut runtime = Runtime::new();
        runtime.load(project.code.clone());

        Self {
            project,
            runtime,
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.project);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // TOOD: Wait this is stupid on mobile
        let mut force_step = ctx.input(|r| r.key_pressed(Key::E) && r.modifiers.ctrl);
        let mut reset_state = ctx.input(|r| r.key_pressed(Key::R) && r.modifiers.ctrl);
        let mut force_save = ctx.input(|r| r.key_pressed(Key::S) && r.modifiers.ctrl);

        TopBottomPanel::top("toope").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Project", |ui| {
                    force_save |= ui.button("Save (CTRL + S)").clicked();
                });

                ui.menu_button("Run", |ui| {
                    ui.menu_button("Mode", |ui| self.project.run_mode.show(ui));
                    force_step |= ui.button("Step (CTRL + E)").clicked();
                });

                ui.menu_button("State", |ui| {
                    reset_state |= ui.button("Reset (CTRL + R)").clicked();
                });

            });
        });

        if force_save {
            if let Some(storage) = frame.storage_mut() {
                self.save(storage);
            }
        }

        if reset_state {
            self.runtime.reset_state();
        }

        let mut changed = false;
        SidePanel::left("leeft").show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                changed |=
                    code_editor_with_autoindent(ui, "CodeEditor".into(), &mut self.project.code, "py")
                        .changed();
            });
        });

        if changed {
            //let start = Instant::now();
            self.runtime.load(self.project.code.clone());
            //println!("Load took {}s", start.elapsed().as_secs_f32());
        };

        let run_requested = match self.project.run_mode {
            RunMode::Continuous => {
                ctx.request_repaint();
                true
            }
            RunMode::Manual => false,
            RunMode::OnScreenUpdate => true,
            RunMode::OnCodeChange => changed,
        };

        CentralPanel::default().show(ctx, |ui| {
            //let (resp, paint) = ui.allocate_painter(egui::Vec2::splat(200.), egui::Sense::click());
            //paint.line_segment([egui::Pos2::ZERO, egui::Pos2::new(1e3, 1e3)], egui::Stroke::new(1., egui::Color32::BLUE));
            //paint.circle_filled(egui::Pos2::ZERO, 1000.0, egui::Color32::BLUE);

            ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.project.run_mode.show_run_button() {
                        if ui.button("Run").clicked() {
                            force_step = true;
                        }
                    }

                    ui.strong("GUI");
                    self.runtime.set_egui(ui);
                    if run_requested || force_step {
                        self.runtime.run_loaded_code();
                    }
                    self.runtime.take_up_egui_space(ui);

                    ui.strong("Console out:");
                    if let Some(error) = self.runtime.error() {
                        ui.label(RichText::new(error).color(Color32::LIGHT_RED));
                    } else {
                        ui.label(RichText::new(self.runtime.stdout().borrow().as_str()).code());
                    }

                });
        });

        //let start = Instant::now();
        //println!("Run took {}ms", (start.elapsed().as_secs_f32() * 1000.0).floor());
    }
}

fn code_editor_with_autoindent(
    ui: &mut Ui,
    id: Id,
    code: &mut String,
    lang: &'static str,
) -> Response {
    let mut layouter = move |ui: &Ui, string: &str, wrap_width: f32| {
        let mut layout_job = highlight(
            ui.ctx(),
            ui.style(),
            &CodeTheme::from_style(ui.style()),
            string,
            lang,
        );

        layout_job.wrap.max_width = wrap_width;
        ui.fonts(|f| f.layout_job(layout_job.clone()))
    };

    let ret = TextEdit::multiline(code)
        .id(id)
        .desired_width(f32::INFINITY)
        .desired_rows(50)
        .code_editor()
        .layouter(&mut layouter)
        .show(ui);

    // Did we make a new line?
    if ret.response.changed() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(cursor) = ret.cursor_range {
            let cursor = cursor.primary.ccursor;

            let prev_newline_idx = code[..cursor.index - 1].rfind('\n');

            if cursor.prefer_next_row {
                if let Some(prev) = prev_newline_idx {
                    // Find the indent
                    let indent_chars: String = code[prev..cursor.index]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .filter(|c| *c == ' ' || *c == '\t')
                        .collect();

                    // Insert indent
                    code.insert_str(cursor.index, &indent_chars);

                    // Set the new cursor pos
                    let mut new_cursor_range = cursor;
                    new_cursor_range.index += indent_chars.len();
                    let mut new_state = ret.state;
                    new_state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(new_cursor_range)));
                    TextEdit::store_state(ui.ctx(), id, new_state);
                }
            }
        }
    }

    ret.response
}

#[derive(serde::Deserialize, serde::Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    Continuous,
    #[default]
    OnScreenUpdate,
    OnCodeChange,
    Manual,
}

impl RunMode {
    fn show(&mut self, ui: &mut Ui) {
        ui.selectable_value(self, Self::Continuous, "Continuous");
        ui.selectable_value(self, Self::OnScreenUpdate, "On Screen Update");
        ui.selectable_value(self, Self::OnCodeChange, "On Code Change");
        ui.selectable_value(self, Self::Manual, "Manual");
    }

    fn show_run_button(&self) -> bool {
        matches!(self, Self::OnCodeChange | Self::Manual)
    }
}
