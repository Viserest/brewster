//! A wasm engine for the browser: [`WasmtimeRunner`] runs page modules in wasmtime.
//!
//! This crate is only the thin part that touches wasmtime. The page protocol (version
//! check, decoding the tree, clicks, problem lines) is `host::WasmGuest`, which is tested
//! without an engine; here an [`Instance`] is made of the module's four exports and its
//! memory.
//!
//! A page gets no capabilities: the module is instantiated with an empty linker, so a
//! module that imports anything (files, sockets, clocks) is refused. Each call into the
//! page has a fuel budget, so an endless loop traps instead of hanging the browser, and
//! the memory it may hold is capped.

use host::{Guest, Host, Instance, Runner, WasmGuest};
use wasmtime::{
    Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};

/// What one page may use.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Fuel for each call into the page (about one unit per wasm instruction). Running
    /// out traps the call.
    pub fuel: u64,
    /// The most linear memory the module may have, in bytes.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            fuel: 1_000_000_000,
            memory_bytes: 128 * 1024 * 1024,
        }
    }
}

/// Starts page modules in wasmtime. Cheap to share: one engine serves every page.
pub struct WasmtimeRunner {
    engine: Engine,
    limits: Limits,
}

impl WasmtimeRunner {
    pub fn new() -> Result<WasmtimeRunner, String> {
        WasmtimeRunner::with_limits(Limits::default())
    }

    pub fn with_limits(limits: Limits) -> Result<WasmtimeRunner, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(err)?;
        Ok(WasmtimeRunner { engine, limits })
    }
}

/// The compiler from the environment plus this engine: what the apps hand to the browser.
pub fn host() -> Result<Host, String> {
    Ok(Host::new(
        plugin::Compiler::detect(),
        Box::new(WasmtimeRunner::new()?),
    ))
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

impl Runner for WasmtimeRunner {
    fn instantiate(&self, wasm: &[u8]) -> Result<Box<dyn Guest>, String> {
        let module = Module::new(&self.engine, wasm).map_err(|e| format!("bad module: {}", err(e)))?;

        let mut store = Store::new(
            &self.engine,
            State {
                limits: StoreLimitsBuilder::new()
                    .memory_size(self.limits.memory_bytes)
                    .instances(1)
                    .memories(1)
                    .trap_on_grow_failure(true)
                    .build(),
            },
        );
        store.limiter(|s| &mut s.limits);
        store.set_fuel(self.limits.fuel).map_err(err)?;

        // No imports are provided: a page that asks for the outside world cannot start.
        let linker: Linker<State> = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("cannot start module: {}", err(e)))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or("not a cre page module: no exported memory")?;
        let export = |what: &str| format!("not a cre page module: no usable export `{what}`");
        let abi = instance
            .get_typed_func::<(), u32>(&mut store, "cre_abi")
            .map_err(|_| export("cre_abi"))?;
        let page = instance
            .get_typed_func::<(), u64>(&mut store, "cre_page")
            .map_err(|_| export("cre_page"))?;
        let event = instance
            .get_typed_func::<u32, u32>(&mut store, "cre_event")
            .map_err(|_| export("cre_event"))?;
        let problems = instance
            .get_typed_func::<(), u64>(&mut store, "cre_problems")
            .map_err(|_| export("cre_problems"))?;
        let alloc = instance
            .get_typed_func::<u32, u32>(&mut store, "cre_alloc")
            .map_err(|_| export("cre_alloc"))?;
        let set_url = instance
            .get_typed_func::<u32, ()>(&mut store, "cre_set_url")
            .map_err(|_| export("cre_set_url"))?;
        let input = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "cre_input")
            .map_err(|_| export("cre_input"))?;
        let net_take = instance
            .get_typed_func::<(), u64>(&mut store, "cre_net_take")
            .map_err(|_| export("cre_net_take"))?;
        let net_done = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "cre_net_done")
            .map_err(|_| export("cre_net_done"))?;

        let guest = WasmGuest::new(Running {
            store,
            memory,
            fuel: self.limits.fuel,
            abi,
            page,
            event,
            problems,
            alloc,
            set_url,
            input,
            net_take,
            net_done,
        })?;
        Ok(Box::new(guest))
    }
}

struct State {
    limits: StoreLimits,
}

/// One module instance: its store, its memory and its exports.
struct Running {
    store: Store<State>,
    memory: Memory,
    fuel: u64,
    abi: TypedFunc<(), u32>,
    page: TypedFunc<(), u64>,
    event: TypedFunc<u32, u32>,
    problems: TypedFunc<(), u64>,
    alloc: TypedFunc<u32, u32>,
    set_url: TypedFunc<u32, ()>,
    input: TypedFunc<(u32, u32), ()>,
    net_take: TypedFunc<(), u64>,
    net_done: TypedFunc<(u32, u32), ()>,
}

impl Running {
    /// Every call starts with a full budget.
    fn refuel(&mut self) -> Result<(), String> {
        self.store.set_fuel(self.fuel).map_err(err)
    }
}

impl Instance for Running {
    fn abi(&mut self) -> Result<u32, String> {
        self.refuel()?;
        self.abi.call(&mut self.store, ()).map_err(err)
    }

    fn page(&mut self) -> Result<u64, String> {
        self.refuel()?;
        self.page.call(&mut self.store, ()).map_err(err)
    }

    fn event(&mut self, id: u32) -> Result<u32, String> {
        self.refuel()?;
        self.event.call(&mut self.store, id).map_err(err)
    }

    fn problems(&mut self) -> Result<u64, String> {
        self.refuel()?;
        self.problems.call(&mut self.store, ()).map_err(err)
    }

    fn read(&mut self, ptr: u32, len: u32) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; len as usize];
        self.memory
            .read(&self.store, ptr as usize, &mut buf)
            .map_err(|_| format!("the page pointed outside its memory ({ptr}+{len})"))?;
        Ok(buf)
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), String> {
        let len = u32::try_from(bytes.len()).map_err(|_| "message too large".to_string())?;
        self.refuel()?;
        let ptr = self.alloc.call(&mut self.store, len).map_err(err)?;
        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .map_err(|_| format!("the page's buffer is outside its memory ({ptr}+{len})"))
    }

    fn set_url(&mut self, len: u32) -> Result<(), String> {
        self.refuel()?;
        self.set_url.call(&mut self.store, len).map_err(err)
    }

    fn input(&mut self, id: u32, len: u32) -> Result<(), String> {
        self.refuel()?;
        self.input.call(&mut self.store, (id, len)).map_err(err)
    }

    fn net_take(&mut self) -> Result<u64, String> {
        self.refuel()?;
        self.net_take.call(&mut self.store, ()).map_err(err)
    }

    fn net_done(&mut self, id: u32, len: u32) -> Result<(), String> {
        self.refuel()?;
        self.net_done.call(&mut self.store, (id, len)).map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A module written by hand in the text format: one exported memory and the
    // exports, with a page that is empty. `wat` text needs wasmtime's default `wat`
    // feature, which is on.
    const EMPTY_PAGE: &str = r#"
        (module
          (memory (export "memory") 1)
          (func (export "cre_abi") (result i32) i32.const 2)
          (func (export "cre_page") (result i64) i64.const 0)
          (func (export "cre_event") (param i32) (result i32) i32.const 0)
          (func (export "cre_problems") (result i64) i64.const 0)
          (func (export "cre_alloc") (param i32) (result i32) i32.const 1024)
          (func (export "cre_set_url") (param i32))
          (func (export "cre_input") (param i32 i32))
          (func (export "cre_net_take") (result i64) i64.const 0)
          (func (export "cre_net_done") (param i32 i32))
          )
    "#;

    #[test]
    fn a_module_with_the_exports_starts() {
        let r = WasmtimeRunner::new().unwrap();
        assert!(r.instantiate(EMPTY_PAGE.as_bytes()).is_ok());
    }

    #[test]
    fn the_address_and_an_input_reach_the_module() {
        let r = WasmtimeRunner::new().unwrap();
        let mut g = r.instantiate(EMPTY_PAGE.as_bytes()).unwrap();
        // The empty page ignores both, but the buffer is allocated, written and passed on.
        g.set_url(&cre::Url::parse("http://a.b/c")).unwrap();
        g.set_input(3, "typed").unwrap();
        assert!(g.net_take().unwrap().is_empty());
    }

    #[test]
    fn a_module_without_the_exports_is_refused() {
        let r = WasmtimeRunner::new().unwrap();
        let e = r.instantiate(b"(module (memory (export \"memory\") 1))").err().unwrap();
        assert!(e.contains("cre_abi"), "{e}");
        assert!(r.instantiate(b"not wasm").is_err());
    }

    #[test]
    fn imports_are_not_provided() {
        let r = WasmtimeRunner::new().unwrap();
        let wat = r#"(module (import "env" "open" (func)) (memory (export "memory") 1))"#;
        assert!(r.instantiate(wat.as_bytes()).is_err());
    }

    #[test]
    fn an_endless_loop_runs_out_of_fuel() {
        let r = WasmtimeRunner::with_limits(Limits { fuel: 10_000, ..Limits::default() }).unwrap();
        let wat = r#"
            (module
              (memory (export "memory") 1)
              (func (export "cre_abi") (result i32) (loop (br 0)) i32.const 1)
              (func (export "cre_page") (result i64) i64.const 0)
              (func (export "cre_event") (param i32) (result i32) i32.const 0)
              (func (export "cre_problems") (result i64) i64.const 0)
              (func (export "cre_alloc") (param i32) (result i32) i32.const 1024)
              (func (export "cre_set_url") (param i32))
              (func (export "cre_input") (param i32 i32))
              (func (export "cre_net_take") (result i64) i64.const 0)
              (func (export "cre_net_done") (param i32 i32))
              )
        "#;
        // `WasmGuest::new` calls `cre_abi`, which never returns: it must trap, not hang.
        assert!(r.instantiate(wat.as_bytes()).is_err());
    }

    #[test]
    fn memory_over_the_cap_is_refused() {
        let r = WasmtimeRunner::with_limits(Limits { memory_bytes: 65536, ..Limits::default() }).unwrap();
        let wat = EMPTY_PAGE.replace("(memory (export \"memory\") 1)", "(memory (export \"memory\") 4)");
        assert!(r.instantiate(wat.as_bytes()).is_err());
    }
}
