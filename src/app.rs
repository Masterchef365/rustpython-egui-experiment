use core::f32;
use std::cell::RefCell;
use std::rc::Rc;

use egui::{CentralPanel, Color32, Id, Response, RichText, ScrollArea, SidePanel, TextEdit, Ui};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};
//use egui_extras::syntax_highlighting::{highlight, CodeTheme};
use rustpython_vm::Interpreter;
use rustpython_vm::{builtins::PyFunction, compiler::Mode, object::Traverse};

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    code: String,

    #[serde(skip)]
    output: Rc<RefCell<String>>,
    #[serde(skip)]
    error: Option<String>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            code: "write(\"Hello World!\")".to_string(),
            output: Default::default(),
            error: None,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut code = String::new();
        if let Some(storage) = cc.storage {
            code = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Self {
            code,
            output: Default::default(),
            error: None,
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.code);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut changed = false;

        SidePanel::left("leeft").show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                changed |=
                    code_editor_with_autoindent(ui, "CodeEditor".into(), &mut self.code, "py")
                        .changed();
            });
        });

        if changed {
            Interpreter::without_stdlib(Default::default()).enter(|vm| {
                let scope = vm.new_scope_with_builtins();

                self.output.borrow_mut().clear();
                self.error = None;

                //let sys = vm.import("sys", 0).unwrap();
                //let stdout = sys.get_item("stdout", vm).unwrap();

                let output_c = self.output.clone();
                let writer = vm.new_function("write", move |s: String| {
                    *output_c.borrow_mut() += &s;
                });

                //sys.del_item("stdout", vm).unwrap();
                //stdout.set_item("write", writer.into(), vm).unwrap();
                scope.globals.set_item("write", writer.into(), vm).unwrap();

                let code_obj = vm.compile(&self.code, Mode::Exec, "<embedded>".to_owned()); //.map_err(|err| vm.new_syntax_error(&err, Some(&code)));
                match code_obj {
                    Ok(obj) => {
                        /*
                        let clear_code = [
                            0x1b, 0x5b, 0x48, 0x1b, 0x5b, 0x32, 0x4a, 0x1b, 0x5b, 0x33, 0x4a,
                        ];
                        let _ = std::io::stdout().write_all(&clear_code);
                        let _ = std::io::stdout().flush();
                        */
                        if let Err(exec_err) = vm.run_code_obj(obj, scope) {
                            let mut s = String::new();
                            vm.write_exception(&mut s, &exec_err).unwrap();
                            self.error = Some(s);
                        }
                    }
                    Err(compile_err) => {
                        self.error = Some(format!("{:#?}", compile_err));
                    }
                }
            })
        };

        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(error) = &self.error {
                        ui.label(RichText::new(error).color(Color32::LIGHT_RED));
                    } else {
                        ui.label(RichText::new("Success").color(Color32::LIGHT_GREEN));
                        ui.label(RichText::new(self.output.borrow().as_str()).code());
                    }
                });
        });
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
