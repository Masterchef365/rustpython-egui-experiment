#![warn(clippy::all, rust_2018_idioms)]

mod app;
use std::{cell::RefCell, rc::Rc};

pub use app::TemplateApp;
use rustpython_vm::{
    builtins::{PyCode, PyType}, compiler::Mode, import::import_source, scope::Scope, types::Constructor, Interpreter, PyRef
};

struct Runtime {
    interpreter: Interpreter,
    scope: Scope,
    output: Rc<RefCell<String>>,
    error: Option<String>,
    code: String,
    code_obj: Option<PyRef<PyCode>>,
}

impl Runtime {
    pub fn new() -> Self {
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            vm.add_native_modules(rustpython_stdlib::get_module_inits());
        });

        let output = Rc::new(RefCell::new(String::new()));

        let scope = interpreter.enter(|vm| {
            // Create scope
            let scope = vm.new_scope_with_builtins();

            // Set stdout hook
            let sys = vm.import("sys", 0).unwrap();

            let py_type = vm.builtins.get_attr("type", vm).unwrap();
            let stdout = py_type.call(("InternalStdout", vm.ctx.new_tuple(vec![]), vm.ctx.new_dict()), vm).unwrap();

            let output_c = output.clone();
            let writer = vm.new_function("write", move |s: String| {
                *output_c.borrow_mut() += &s;
            });

            stdout.set_attr("write", writer, vm).unwrap();

            sys.set_attr("stdout", stdout.clone(), vm).unwrap();


            // Import a library
            if let Err(e) = import_source(vm, "euclid", include_str!("./euclid/euclid.py")) {
                let mut s = String::new();
                vm.write_exception(&mut s, &e).unwrap();
                panic!("{}", s);
            }

            scope
        });

        Self {
            code: "".into(),
            interpreter,
            scope,
            output,
            error: None,
            code_obj: None,
        }
    }

    pub fn load(&mut self, code: String) {
        let scope = self.scope.clone();
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
