#![warn(clippy::all, rust_2018_idioms)]

mod app;
use std::{cell::RefCell, rc::Rc};

pub use app::TemplateApp;
use egui::{Painter, Pos2, Stroke, Ui};
use rust_py_module::{PyEgui, PyResponse};
use rustpython_vm::{
    builtins::{PyCode, PyFloat, PyStrRef, PyType},
    compiler::Mode,
    function::IntoPyNativeFn,
    import::import_source,
    pyclass, pymodule,
    scope::Scope,
    Interpreter, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
};

struct Runtime {
    interpreter: Interpreter,
    scope: Scope,
    output: Rc<RefCell<String>>,
    error: Option<String>,
    code: String,
    code_obj: Option<PyRef<PyCode>>,
    child_ui: Option<Rc<RefCell<Ui>>>,
}

//use rust_py_module::PyEguiResponse;

trait UnwrapException<T> {
    fn unwrap_exception(self, vm: &VirtualMachine) -> T;
}

impl<T> UnwrapException<T> for PyResult<T> {
    #[track_caller]
    fn unwrap_exception(self, vm: &VirtualMachine) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                let mut s = String::new();
                vm.write_exception(&mut s, &e)
                    .expect("Failed to write exception");
                panic!("{}", s);
            }
        }
    }
}

fn anon_object(vm: &VirtualMachine, name: &str) -> PyObjectRef {
    let py_type = vm.builtins.get_attr("type", vm).unwrap_exception(vm);
    let args = (name, vm.ctx.new_tuple(vec![]), vm.ctx.new_dict());
    py_type.call(args, vm).unwrap_exception(vm)
}

impl Runtime {
    pub fn new() -> Self {
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            vm.add_native_modules(rustpython_stdlib::get_module_inits());
            vm.add_native_module(
                "rust_py_module".to_owned(),
                Box::new(rust_py_module::make_module),
            );
        });

        let output = Rc::new(RefCell::new(String::new()));

        let scope = interpreter.enter(|vm| {
            // Create scope
            let scope = vm.new_scope_with_builtins();

            // Set stdout hook
            let sys = vm.import("sys", 0).unwrap_exception(vm);
            let _ = vm.import("rust_py_module", 0).unwrap_exception(vm);

            let stdout = anon_object(vm, "InternalStdout");

            let output_c = output.clone();
            let writer = vm.new_function("write", move |s: String| {
                *output_c.borrow_mut() += &s;
            });

            stdout.set_attr("write", writer, vm).unwrap_exception(vm);

            sys.set_attr("stdout", stdout.clone(), vm)
                .unwrap_exception(vm);

            // Import a library
            import_source(vm, "euclid", include_str!("./euclid/euclid.py")).unwrap_exception(vm);

            scope
        });

        Self {
            child_ui: None,
            code: r#"# The same scope is used each frame (unless reset)
# So we can declare variables using something like:
try: s
except: s = "type here"

# Then we can just call some builtin functions
s, resp = egui.text_edit_singleline(s)

# Including simulated stdout
print("Hello, world!")
print()
print(f"You cannot {s}")"#
                .into(),
            interpreter,
            scope,
            output,
            error: None,
            code_obj: None,
        }
    }

    pub fn load(&mut self, code: String) {
        self.interpreter.enter(|vm| {
            let code_obj = vm.compile(&code, Mode::Exec, "the code you just wrote in the thingy".to_owned());
            match code_obj {
                Ok(obj) => {
                    self.code_obj = Some(obj);
                }
                Err(compile_err) => {
                    self.error = Some(format!("{:#?}", compile_err));
                }
            }
        });
        self.code = code;
    }

    pub fn run_loaded_code(&mut self) {
        let Some(code) = self.code_obj.clone() else {
            return;
        };

        self.output.borrow_mut().clear();
        self.error = None;

        let scope = self.scope.clone();
        self.error = self.interpreter.enter(move |vm| {
            if let Err(exec_err) = vm.run_code_obj(code, scope) {
                let mut s = String::new();
                vm.write_exception(&mut s, &exec_err).unwrap();
                Some(s)
            } else {
                None
            }
        });
    }

    pub fn set_egui(&mut self, ui: &mut Ui) {
        let ui = Rc::new(RefCell::new(ui.new_child(Default::default())));
        self.child_ui = Some(ui.clone());

        let scope = self.scope.clone();
        self.interpreter.enter(move |vm| {
            let py_ui = vm.new_pyobj(PyEgui { ui });
            scope
                .globals
                .set_item("egui", py_ui, vm)
                .unwrap_exception(vm);
        });
    }

    pub fn take_up_egui_space(&self, ui: &mut Ui) {
        if let Some(child) = &self.child_ui {
            let desired_size = child.borrow_mut().min_size();
            ui.allocate_space(desired_size);
        }
    }

    pub fn reset_state(&mut self) {
        let old = std::mem::replace(self, Self::new());
        self.load(old.code);
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(|x| x.as_str())
    }

    pub fn stdout(&mut self) -> Rc<RefCell<String>> {
        self.output.clone()
    }
}

#[pymodule]
mod rust_py_module {
    use egui::Align2;
    use rustpython_vm::builtins::PyBaseExceptionRef;

    use super::*;

    #[pyattr]
    #[derive(PyPayload, Clone)]
    #[pyclass(module = "rust_py_module", name = "PyEgui")]
    pub struct PyEgui {
        pub ui: Rc<RefCell<Ui>>,
    }

    fn parse_align2_from_str(
        s: &str,
        vm: &VirtualMachine,
    ) -> Result<egui::Align2, PyBaseExceptionRef> {
        match s.to_uppercase().as_str() {
            "LEFT_BOTTOM" => Ok(Align2::LEFT_BOTTOM),
            "LEFT_CENTER" => Ok(Align2::LEFT_CENTER),
            "LEFT_TOP" => Ok(Align2::LEFT_TOP),
            "CENTER_BOTTOM" => Ok(Align2::CENTER_BOTTOM),
            "" | "CENTER_CENTER" => Ok(Align2::CENTER_CENTER),
            "CENTER_TOP" => Ok(Align2::CENTER_TOP),
            "RIGHT_BOTTOM" => Ok(Align2::RIGHT_BOTTOM),
            "RIGHT_CENTER" => Ok(Align2::RIGHT_CENTER),
            "RIGHT_TOP" => Ok(Align2::RIGHT_TOP),
            _ => Err(vm.new_exception_msg(
                vm.ctx.exceptions.runtime_error.to_owned(),
                "Must be {LEFT,CENTER,RIGHT}_{TOP,CENTER,BOTTOM}".to_string(),
            )),
        }
    }

    fn parse_sense_from_str(
        s: &str,
        vm: &VirtualMachine,
    ) -> Result<egui::Sense, PyBaseExceptionRef> {
        let mut sense = egui::Sense {
            click: false,
            drag: false,
            focusable: false,
        };

        for substr in s.split('|') {
            match substr {
                "click" => sense.click = true,
                "drag" => sense.drag = true,
                "focusable" => sense.focusable = true,
                _ => {
                    return Err(vm.new_exception_msg(
                        vm.ctx.exceptions.runtime_error.to_owned(),
                        "Must be click, drag, or focusable".to_string(),
                    ));
                }
            }
        }

        Ok(sense)
    }

    fn parse_color(
        color: &Vec<u8>,
        vm: &VirtualMachine,
    ) -> Result<egui::Color32, PyBaseExceptionRef> {
        if color.len() != 4 {
            Err(vm.new_exception_msg(
                vm.ctx.exceptions.runtime_error.to_owned(),
                "Colors are from premultiplied RGBA".to_owned(),
            ))
        } else {
            Ok(egui::Color32::from_rgba_premultiplied(
                color[0], color[1], color[2], color[3],
            ))
        }
    }

    fn parse_vec2(value: &Vec<f32>, vm: &VirtualMachine) -> Result<egui::Vec2, PyBaseExceptionRef> {
        if value.len() != 2 {
            Err(vm.new_exception_msg(
                vm.ctx.exceptions.runtime_error.to_owned(),
                "Points must be of dimension 2".to_owned(),
            ))
        } else {
            Ok(egui::Vec2::new(value[0], value[1]))
        }
    }

    fn parse_pos2(value: &Vec<f32>, vm: &VirtualMachine) -> Result<egui::Pos2, PyBaseExceptionRef> {
        parse_vec2(value, vm).map(|v| v.to_pos2())
    }

    #[pyclass]
    impl PyEgui {
        #[pymethod]
        fn button(&self, text: PyStrRef) -> PyResponse {
            PyResponse::from(self.ui.borrow_mut().button(text.as_str()))
        }

        #[pymethod]
        fn text_edit_singleline(&self, text: PyStrRef) -> (String, PyResponse) {
            let mut editable = text.to_string();
            let ret = self.ui.borrow_mut().text_edit_singleline(&mut editable);
            (editable, PyResponse::from(ret))
        }

        #[pymethod]
        fn painter(&self) -> PyPainter {
            PyPainter {
                paint: self.ui.borrow().painter().clone(),
            }
        }

        #[pymethod]
        fn allocate_painter(
            &self,
            desired_size: Vec<f32>,
            sense: String,
            vm: &VirtualMachine,
        ) -> Result<(PyResponse, PyPainter), PyBaseExceptionRef> {
            let sense = parse_sense_from_str(&sense, vm)?;
            let desired_size = parse_vec2(&desired_size, vm)?;

            let (resp, paint) = self.ui.borrow_mut().allocate_painter(desired_size, sense);

            Ok((PyResponse { resp }, PyPainter { paint }))
        }
    }

    impl std::fmt::Debug for PyEgui {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let ptr = self.ui.as_ptr();
            writeln!(f, "PyEgui {:?}", ptr)
        }
    }

    #[pyattr]
    #[pyclass(module = "rust_py_module", name = "PyResponse")]
    #[derive(Debug, PyPayload)]
    pub struct PyResponse {
        pub resp: egui::Response,
    }

    #[pyattr]
    #[pyclass(module = "rust_py_module", name = "PyRect")]
    #[derive(Debug, PyPayload)]
    pub struct PyRect {
        pub rect: egui::Rect,
    }

    #[pyclass]
    impl PyResponse {
        #[pymethod]
        fn clicked(&self) -> bool {
            self.resp.clicked()
        }

        #[pymethod]
        fn rect(&self) -> PyRect {
            PyRect {
                rect: self.resp.rect,
            }
        }
    }

    #[pyclass]
    impl PyRect {
        #[pymethod]
        fn min(&self) -> (f32, f32) {
            (self.rect.min.x, self.rect.min.y)
        }

        #[pymethod]
        fn max(&self) -> (f32, f32) {
            (self.rect.max.x, self.rect.max.y)
        }
    }

    impl From<egui::Response> for PyResponse {
        fn from(resp: egui::Response) -> Self {
            Self { resp }
        }
    }

    #[pyattr]
    #[pyclass(module = "rust_py_module", name = "PyPainter")]
    #[derive(PyPayload)]
    pub struct PyPainter {
        pub paint: egui::Painter,
    }

    #[pyclass]
    impl PyPainter {
        #[pymethod]
        fn line(
            &self,
            points: Vec<Vec<f32>>,
            stroke_width: f32,
            color: Vec<u8>,
            vm: &VirtualMachine,
        ) -> Result<(), PyBaseExceptionRef> {
            let color = parse_color(&color, vm)?;
            for pair in points.windows(2) {
                self.paint.line_segment(
                    [parse_pos2(&pair[0], vm)?, parse_pos2(&pair[1], vm)?],
                    Stroke::new(stroke_width, color),
                );
            }

            Ok(())
        }

        #[pymethod]
        fn circle(
            &self,
            center: Vec<f32>,
            radius: f32,
            fill_color: Vec<u8>,
            stroke_width: f32,
            stroke_color: Vec<u8>,
            vm: &VirtualMachine,
        ) -> Result<(), PyBaseExceptionRef> {
            self.paint.circle(
                parse_pos2(&center, vm)?,
                radius,
                parse_color(&fill_color, vm)?,
                Stroke::new(stroke_width, parse_color(&stroke_color, vm)?),
            );
            Ok(())
        }

        #[pymethod]
        fn text(
            &self,
            pos: Vec<f32>,
            anchor: String,
            text: PyStrRef,
            text_color: Vec<u8>,
            vm: &VirtualMachine,
        ) -> Result<PyRect, PyBaseExceptionRef> {
            let rect = self.paint.text(
                parse_pos2(&pos, vm)?,
                parse_align2_from_str(&anchor, vm)?,
                text.as_str(),
                Default::default(),
                parse_color(&text_color, vm)?,
            );
            Ok(PyRect { rect })
        }
    }

    impl std::fmt::Debug for PyPainter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "PyPainter")
        }
    }
}
