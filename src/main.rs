use eframe::{
    egui::{CentralPanel, ScrollArea, SidePanel, TextEdit, TopBottomPanel, Ui, Vec2},
    NativeOptions,
};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};

fn main() {
    let mut code = String::new();
    let mut lang = String::new();

    eframe::run_simple_native("datathing", NativeOptions::default(), move |ctx, _frame| {
        let width = ctx.available_rect().width();
        SidePanel::left("leeft")
            .show(ctx, |ui| {
                let lang = lang.clone();
                let mut layouter = move |ui: &Ui, string: &str, wrap_width: f32| {
                    let mut layout_job = highlight(
                        ui.ctx(),
                        ui.style(),
                        &CodeTheme::from_style(ui.style()),
                        string,
                        &lang
                    );

                    layout_job.wrap.max_width = wrap_width;
                    ui.fonts(|f| f.layout_job(layout_job.clone()))
                };

                ScrollArea::vertical().show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut code)
                            .desired_width(f32::INFINITY)
                            .desired_rows(50)
                            .layouter(&mut layouter)
                    );
                });
            });

        CentralPanel::default().show(ctx, |ui| {
            ui.label("Language");
            ui.text_edit_singleline(&mut lang);
        });
    })
    .unwrap();
}
