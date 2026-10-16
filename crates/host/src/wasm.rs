//! The page protocol over a wasm instance, independent of the wasm engine.
//!
//! A real engine (see `crates/host-wasmtime`) only has to implement [`Instance`]: call the
//! four exports of `cre::abi` and read a range of the module's memory. Everything else,
//! the version check, decoding the tree, clicks and problem lines, is here, so it is
//! tested without any engine.

use crate::Guest;

/// The largest tree or problem list a page may hand out. A bigger one is a broken page.
pub const MAX_OUTPUT: u32 = 64 * 1024 * 1024;

/// A running module, as the engine exposes it. Each method is one export of `cre::abi`.
pub trait Instance {
    /// `cre_abi() -> u32`
    fn abi(&mut self) -> Result<u32, String>;
    /// `cre_page() -> u64`: `(ptr << 32) | len`
    fn page(&mut self) -> Result<u64, String>;
    /// `cre_event(id: u32) -> u32`
    fn event(&mut self, id: u32) -> Result<u32, String>;
    /// `cre_problems() -> u64`: `(ptr << 32) | len`
    fn problems(&mut self) -> Result<u64, String>;
    /// `len` bytes of the module's memory at `ptr`; an error when out of range.
    fn read(&mut self, ptr: u32, len: u32) -> Result<Vec<u8>, String>;
    /// Puts `bytes` in the module's input buffer: `cre_alloc(len)`, then a copy into its
    /// memory.
    fn load(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// `cre_set_url(len)`: the buffer holds a method byte and the address.
    fn set_url(&mut self, len: u32) -> Result<(), String>;
    /// `cre_input(id, len)`: the buffer holds the input's new value.
    fn input(&mut self, id: u32, len: u32) -> Result<(), String>;
    /// `cre_net_take() -> u64`: `(ptr << 32) | len` of the queued requests.
    fn net_take(&mut self) -> Result<u64, String>;
    /// `cre_net_done(id, len)`: the buffer holds the answer.
    fn net_done(&mut self, id: u32, len: u32) -> Result<(), String>;
}

/// A [`Guest`] backed by any [`Instance`].
pub struct WasmGuest<I: Instance> {
    inst: I,
}

impl<I: Instance> WasmGuest<I> {
    /// Checks that the module speaks this version of the page interface.
    pub fn new(mut inst: I) -> Result<WasmGuest<I>, String> {
        let v = inst
            .abi()
            .map_err(|e| format!("not a cre page module: {}", e))?;
        if v != cre::abi::VERSION {
            return Err(format!(
                "the page module uses interface version {} but this browser speaks {}",
                v,
                cre::abi::VERSION
            ));
        }
        Ok(WasmGuest { inst })
    }

    fn fetch(&mut self, packed: u64) -> Result<Vec<u8>, String> {
        let (ptr, len) = ((packed >> 32) as u32, packed as u32);
        if len > MAX_OUTPUT {
            return Err(format!("the page returned {} bytes, more than the limit", len));
        }
        self.inst.read(ptr, len)
    }
}

impl<I: Instance> Guest for WasmGuest<I> {
    fn tree(&mut self) -> Result<Vec<tree::Element>, String> {
        let packed = self.inst.page()?;
        let bytes = self.fetch(packed)?;
        tree::wire::decode(&bytes).map_err(|e| format!("bad page from module: {}", e))
    }

    fn click(&mut self, id: usize) -> Result<bool, String> {
        let Ok(id) = u32::try_from(id) else {
            return Ok(false);
        };
        Ok(self.inst.event(id)? == 1)
    }

    fn set_url(&mut self, url: &cre::Url) -> Result<(), String> {
        let mut b = vec![url.method.code()];
        b.extend_from_slice(url.href.as_bytes());
        self.inst.load(&b)?;
        self.inst.set_url(b.len() as u32)
    }

    fn set_input(&mut self, id: usize, value: &str) -> Result<(), String> {
        let Ok(id) = u32::try_from(id) else {
            return Ok(());
        };
        self.inst.load(value.as_bytes())?;
        self.inst.input(id, value.len() as u32)
    }

    fn net_take(&mut self) -> Result<Vec<cre::NetRequest>, String> {
        let packed = self.inst.net_take()?;
        let bytes = self.fetch(packed)?;
        cre::decode_requests(&bytes).map_err(|e| format!("bad requests from module: {}", e))
    }

    fn net_done(&mut self, id: u32, result: cre::NetResult) -> Result<(), String> {
        let b = cre::encode_result(&result);
        self.inst.load(&b)?;
        self.inst.net_done(id, b.len() as u32)
    }

    fn problems(&mut self) -> Vec<String> {
        let text = self
            .inst
            .problems()
            .and_then(|p| self.fetch(p))
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        match text {
            Ok(t) => t.lines().filter(|l| !l.is_empty()).map(String::from).collect(),
            Err(e) => vec![e],
        }
    }
}
