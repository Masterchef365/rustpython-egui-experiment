#![warn(clippy::all, rust_2018_idioms)]

mod app;
use std::{borrow::Borrow, cell::RefCell, rc::Rc};

pub use app::TemplateApp;
use egui::{Painter, Pos2, Stroke, Ui};
use rust_py_module::PyEguiResponse;
use rustpython_vm::{
    builtins::{PyCode, PyFloat, PyStrRef, PyType},
    compiler::Mode,
    function::IntoPyNativeFn,
    import::import_source,
    pyclass,
    pymodule,
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
            let egui_obj = anon_object(vm, "EguiIntegration");

            let sub = ui.clone();
            Self::add_egui_fn(vm, egui_obj.clone(), "text_edit_singleline", move |s: PyStrRef| {
                let mut editable = s.to_string();
                let ret = sub.borrow_mut().text_edit_singleline(&mut editable);

                //(editable, PyEguiResponse::from(ret))
                ()
            });

            let sub = ui.clone();
            Self::add_egui_fn(vm, egui_obj.clone(), "button", move |s: PyStrRef| {
                dbg!(PyEguiResponse::from(sub.borrow_mut().button(s.as_str())))
            });

            scope
                .globals
                .set_item("egui", egui_obj, vm)
                .unwrap_exception(vm);
        });
    }

    fn add_egui_fn<F, FKind>(
        vm: &VirtualMachine,
        egui_obj: PyObjectRef,
        name: &'static str,
        func: F,
    ) where
        F: IntoPyNativeFn<FKind>,
    {
        let wrapped = vm.new_function(name, func);
        egui_obj.set_attr(name, wrapped, vm).unwrap_exception(vm);
    }

    pub fn take_up_egui_space(&self, ui: &mut Ui) {
        if let Some(child) = &self.child_ui {
            let desired_size = child.borrow_mut().min_size();
            ui.allocate_space(desired_size);
        }
    }

    /*
    pub fn set_painter(&mut self, painter: Painter) {
        let scope = self.scope.clone();
        self.interpreter.enter(move |vm| {
            vm.new_function("line", |a: PyObjectRef, vm: &VirtualMachine| {
                let x1: f32 = a
                    .get_attr("x", vm)
                    .unwrap_exception(vm)
                    .try_into_value(vm)
                    .unwrap_exception(vm);
                //painter.line_segment([Pos2::new(a[0], a[1]), Pos2::new(b[0], b[1])], Stroke::new(stroke, color.into()));
            });
            //scope.globals.
        });
    }
    */

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
    use super::*;

    #[pyattr]
    #[pyclass(module = "rust_py_module", name = "PyEguiResponse")]
    #[derive(Debug, PyPayload)]
    pub struct PyEguiResponse {
        pub clicked: bool,
    }

    #[pyclass]
    impl PyEguiResponse {
    }


    impl From<egui::Response> for PyEguiResponse {
        fn from(value: egui::Response) -> Self {
            Self {
                clicked: value.clicked(),
            }
        }
    }
}
