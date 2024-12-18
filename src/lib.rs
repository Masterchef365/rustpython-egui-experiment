#![warn(clippy::all, rust_2018_idioms)]

mod app;
use std::{cell::RefCell, rc::Rc};

pub use app::TemplateApp;
use egui::{Painter, Pos2, Stroke, Ui};
use rust_py_module::{PyEgui, PyEguiResponse};
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
            code: "".into(),
            interpreter,
            scope,
            output,
            error: None,
            code_obj: None,
        }
    }

    pub fn load(&mut self, code: String) {
        self.interpreter.enter(|vm| {
            let code_obj = vm.compile(&code, Mode::Exec, "<embedded>".to_owned());
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
    use egui::Color32;
    use rustpython_vm::builtins::{PyList, PyListRef};

    use super::*;

    #[pyattr]
    #[derive(PyPayload, Clone)]
    #[pyclass(module = "rust_py_module", name = "PyEgui")]
    pub struct PyEgui {
        pub ui: Rc<RefCell<Ui>>,
    }

    #[pyclass]
    impl PyEgui {
        #[pymethod]
        fn button(&self, text: PyStrRef) -> PyEguiResponse {
            PyEguiResponse::from(self.ui.borrow_mut().button(text.as_str()))
        }

        #[pymethod]
        fn text_edit_singleline(&self, text: PyStrRef) -> (String, PyEguiResponse) {
            let mut editable = text.to_string();
            let ret = self.ui.borrow_mut().text_edit_singleline(&mut editable);
            (editable, PyEguiResponse::from(ret))
        }

        #[pymethod]
        fn painter(&self) -> PyPainter {
            PyPainter {
                paint: self.ui.borrow().painter().clone(),
            }
        }
    }

    impl std::fmt::Debug for PyEgui {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let ptr = self.ui.as_ptr();
            writeln!(f, "PyEgui {:?}", ptr)
        }
    }

    #[pyattr]
    #[pyclass(module = "rust_py_module", name = "PyEguiResponse")]
    #[derive(Debug, PyPayload)]
    pub struct PyEguiResponse {
        pub resp: egui::Response,
    }

    #[pyclass]
    impl PyEguiResponse {
        #[pymethod]
        fn clicked(&self) -> bool {
            self.resp.clicked()
        }
    }

    impl From<egui::Response> for PyEguiResponse {
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
        ) {
            self.paint.line_segment(
                [
                    Pos2::new(points[0][0], points[0][1]),
                    Pos2::new(points[1][0], points[1][1]),
                ],
                Stroke::new(
                    stroke_width,
                    Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]),
                ),
            );
        }
    }

    impl std::fmt::Debug for PyPainter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "PyPainter")
        }
    }
}
