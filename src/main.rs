use std::io::Write;

use eframe::{
    egui::{
        CentralPanel, Color32, RichText, ScrollArea, SidePanel, TextEdit, TopBottomPanel, Ui, Vec2,
    },
    NativeOptions,
};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};
use rustpython_vm::{builtins::PyFunction, compiler::Mode, object::Traverse};
use rustpython_vm::Interpreter;

fn main() {
    let mut code = String::new();
    let mut output = String::new();

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
                changed |= ui.add(
                    TextEdit::multiline(&mut code)
                    .desired_width(f32::INFINITY)
                    .desired_rows(50)
                    .code_editor()
                    .layouter(&mut layouter),
                ).changed();
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            let output = if changed {
                Interpreter::without_stdlib(Default::default()).enter(|vm| {
                    let scope = vm.new_scope_with_builtins();

                    let sys = vm.import("sys", 0).unwrap();
                    //let stdout = sys.get_item("stdout", vm).unwrap();

                    let writer = vm.new_function("write", |s: String| println!("{}", s));

                    //sys.del_item("stdout", vm).unwrap();
                    //stdout.set_item("write", writer.into(), vm).unwrap();
                    scope.globals.set_item("write", writer.into(), vm).unwrap();

                    let code_obj = vm.compile(&code, Mode::Exec, "<embedded>".to_owned()); //.map_err(|err| vm.new_syntax_error(&err, Some(&code)));
                    match code_obj {
                        Ok(obj) => {
                            let clear_code = [0x1b, 0x5b, 0x48, 0x1b, 0x5b, 0x32, 0x4a, 0x1b, 0x5b, 0x33, 0x4a];
                            let _ = std::io::stdout().write_all(&clear_code);
                            let _ = std::io::stdout().flush();
                            if let Err(e) = vm.run_code_obj(obj, scope) {
                                ui.label(RichText::new(format!("{:#?}", e)).color(Color32::RED));
                                None
                            } else {
                                Some(&output)
                            }
                        }
                        Err(e) => {
                            ui.label(RichText::new(e.to_string()).color(Color32::RED));
                            None
                        }
                    }
                })
            } else {
                Some(&output)
            };

            if let Some(output) = output {
                ui.label(RichText::new("Success").color(Color32::LIGHT_GREEN));
                ui.label(RichText::new(output).code());
            }
        });
    }).unwrap();
}
