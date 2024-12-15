#![warn(clippy::all, rust_2018_idioms)]

mod app;
use std::{cell::RefCell, rc::Rc};

pub use app::TemplateApp;
use rustpython_vm::{compiler::Mode, import::import_source, scope::Scope, Interpreter};

struct Runtime {
    interpreter: Interpreter,
    scope: Scope,
    output: Rc<RefCell<String>>,
    error: Option<String>,
    code: String,
}

impl Runtime {
    pub fn new() -> Self {
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            vm.add_native_modules(rustpython_stdlib::get_module_inits());
        });

        let scope = interpreter.enter(|vm| vm.new_scope_with_builtins());

        Self {
            code: "".into(),
            interpreter,
            scope,
            output: Default::default(),
            error: None,
        }
    }

    pub fn load(&mut self, code: String) {
        let scope = self.scope.clone();
        self.interpreter.enter(|vm| {
            self.output.borrow_mut().clear();
            self.error = None;

            let sys = vm.import("sys", 0).unwrap();
            let stdout = sys.get_attr("stdout", vm).unwrap();

            let output_c = self.output.clone();
            let writer = vm.new_function("write", move |s: String| {
                *output_c.borrow_mut() += &s;
            });

            stdout.set_attr("write", writer, vm).unwrap();

            if let Err(e) = import_source(vm, "euclid", include_str!("./euclid/euclid.py")) {
                let mut s = String::new();
                vm.write_exception(&mut s, &e).unwrap();
                panic!("{}", s);
            }

            let code_obj = vm.compile(&code, Mode::Exec, "<embedded>".to_owned()); 
            match code_obj {
                Ok(obj) => {
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
        });
        self.code = code;
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
