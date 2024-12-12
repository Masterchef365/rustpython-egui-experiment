use std::{cell::RefCell, io::Write, ops::DerefMut, rc::Rc};

use eframe::{
    egui::{
        CentralPanel, Color32, RichText, ScrollArea, SidePanel, TextBuffer, TextEdit,
        TopBottomPanel, Ui, Vec2,
    },
    NativeOptions,
};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};
use rustpython_vm::Interpreter;
use rustpython_vm::{builtins::PyFunction, compiler::Mode, object::Traverse};

fn main() {
    let mut code = String::new();
    let mut output = Rc::new(RefCell::new(String::new()));
    let mut error: Option<String> = None;

    eframe::run_simple_native("datathing", NativeOptions::default(), move |ctx, _frame| {
        let mut changed = false;

        SidePanel::left("leeft").show(ctx, |ui| {
            let mut layouter = move |ui: &Ui, string: &str, wrap_width: f32| {
                let mut layout_job = highlight(
                    ui.ctx(),
                    ui.style(),
                    &CodeTheme::from_style(ui.style()),
                    string,
                    "py",
                );

                layout_job.wrap.max_width = wrap_width;
                ui.fonts(|f| f.layout_job(layout_job.clone()))
            };

            ScrollArea::vertical().show(ui, |ui| {
                changed |= ui
                    .add(
                        TextEdit::multiline(&mut code)
                            .desired_width(f32::INFINITY)
                            .desired_rows(50)
                            .code_editor()
                            .layouter(&mut layouter),
                    )
                    .changed();
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            if changed {
                Interpreter::without_stdlib(Default::default()).enter(|vm| {
                    let scope = vm.new_scope_with_builtins();
                    output.borrow_mut().clear();
                    error = None;

                    //let sys = vm.import("sys", 0).unwrap();
                    //let stdout = sys.get_item("stdout", vm).unwrap();

                    let output_c = output.clone();
                    let writer = vm.new_function("write", move |s: String| {
                        *output_c.borrow_mut() += &s;
                    });

                    //sys.del_item("stdout", vm).unwrap();
                    //stdout.set_item("write", writer.into(), vm).unwrap();
                    scope.globals.set_item("write", writer.into(), vm).unwrap();

                    let code_obj = vm.compile(&code, Mode::Exec, "<embedded>".to_owned()); //.map_err(|err| vm.new_syntax_error(&err, Some(&code)));
                    match code_obj {
                        Ok(obj) => {
                            /*
                            let clear_code = [
                                0x1b, 0x5b, 0x48, 0x1b, 0x5b, 0x32, 0x4a, 0x1b, 0x5b, 0x33, 0x4a,
                            ];
                            let _ = std::io::stdout().write_all(&clear_code);
                            let _ = std::io::stdout().flush();
                            */
                            if let Err(e) = vm.run_code_obj(obj, scope) {
                                error = Some(format!("{:#?}", e));
                            }
                        }
                        Err(e) => {
                            error = Some(format!("{:#?}", e));
                        }
                    }
                })
            };

            if let Some(error) = &error {
                ui.label(RichText::new(error).color(Color32::RED));
            } else {
                ui.label(RichText::new("Success").color(Color32::LIGHT_GREEN));
                ui.label(RichText::new(output.borrow().as_str()).code());
            }
        });
    })
    .unwrap();
}
