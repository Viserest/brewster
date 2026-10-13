//! Turning page source into a wasm module, and getting a module for a URL.
//!
//! Source is compiled with `rustc` directly (no cargo, no dependency graph) against the
//! prebuilt `cre` SDK, so a small page takes about as long as one crate. The result is
//! cached by a hash of everything that influences it, so reloading an unchanged page never
//! compiles again. When compiling is not possible (no `rustc`, no wasm target, a compile
//! error) the loader asks for the pre-compiled `.wasm` instead.

use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use location::{Loader, Url};

use crate::{PluginError, is_wasm};

/// How many characters of compiler output an error keeps.
const MAX_ERROR: usize = 4000;

pub struct Compiler {
    pub rustc: PathBuf,
    /// The target triple modules are built for.
    pub target: String,
    /// Where `libcre.rlib` (built for `target`) lives, with its dependencies in `deps/`.
    pub sdk_dir: Option<PathBuf>,
    /// Compiled modules, one `<hash>.wasm` each.
    pub cache_dir: PathBuf,
    version: RefCell<Option<Result<String, String>>>,
    /// Sources that failed to compile this session, so a broken page is tried once.
    failed: RefCell<HashMap<String, String>>,
}

impl Compiler {
    pub fn new(rustc: impl Into<PathBuf>, sdk_dir: Option<PathBuf>, cache_dir: PathBuf) -> Compiler {
        Compiler {
            rustc: rustc.into(),
            target: "wasm32-unknown-unknown".to_string(),
            sdk_dir,
            cache_dir,
            version: RefCell::new(None),
            failed: RefCell::new(HashMap::new()),
        }
    }

    /// From the environment: `BREWSTER_RUSTC` (default `rustc`), `BREWSTER_SDK_DIR` (default
    /// `sdk/` next to the program, else `target/wasm32-unknown-unknown/release` of the
    /// current directory) and the temporary directory for the cache.
    pub fn detect() -> Compiler {
        let rustc = std::env::var_os("BREWSTER_RUSTC").unwrap_or_else(|| "rustc".into());
        let sdk = std::env::var_os("BREWSTER_SDK_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                let exe = std::env::current_exe().ok()?;
                Some(exe.parent()?.join("sdk"))
            })
            .filter(|p| p.join("libcre.rlib").is_file())
            .or_else(|| {
                let p = PathBuf::from("target/wasm32-unknown-unknown/release");
                p.join("libcre.rlib").is_file().then_some(p)
            });
        Compiler::new(rustc, sdk, std::env::temp_dir().join("brewster").join("modules"))
    }

    fn rustc_version(&self) -> Result<String, String> {
        self.version
            .borrow_mut()
            .get_or_insert_with(|| {
                let out = Command::new(&self.rustc)
                    .arg("--version")
                    .output()
                    .map_err(|e| format!("cannot run {}: {}", self.rustc.display(), e))?;
                if !out.status.success() {
                    return Err(format!("{} --version failed", self.rustc.display()));
                }
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
            })
            .clone()
    }

    /// Everything besides the source that changes the output.
    fn stamp(&self, sdk: &Path) -> Result<String, String> {
        let meta = fs::metadata(sdk.join("libcre.rlib"))
            .map_err(|e| format!("no SDK in {}: {}", sdk.display(), e))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        Ok(format!(
            "{}|{}|{}|{}",
            self.rustc_version()?,
            self.target,
            meta.len(),
            mtime
        ))
    }

    /// The cache key of `source`: 32 hex digits.
    pub fn key(&self, source: &str) -> Result<String, String> {
        let sdk = self.sdk()?;
        Ok(hash128(&[self.stamp(sdk)?.as_bytes(), source.as_bytes()]))
    }

    fn sdk(&self) -> Result<&PathBuf, String> {
        self.sdk_dir.as_ref().ok_or_else(|| {
            "no cre SDK for compiling: build it (`cargo build -p tree -p cre --release --target \
             wasm32-unknown-unknown`) and set BREWSTER_SDK_DIR to the output directory"
                .to_string()
        })
    }

    /// Compiles `source` to a wasm module, or returns the cached one. The error is the
    /// compiler's own message.
    pub fn compile(&self, source: &str) -> Result<Vec<u8>, String> {
        let key = self.key(source)?;
        let wasm = self.cache_dir.join(format!("{key}.wasm"));
        if let Ok(bytes) = fs::read(&wasm) {
            if is_wasm(&bytes) {
                return Ok(bytes);
            }
        }
        if let Some(e) = self.failed.borrow().get(&key) {
            return Err(e.clone());
        }
        match self.run_rustc(&key, source, &wasm) {
            Ok(bytes) => Ok(bytes),
            Err(e) => {
                self.failed.borrow_mut().insert(key, e.clone());
                Err(e)
            }
        }
    }

    fn run_rustc(&self, key: &str, source: &str, wasm: &Path) -> Result<Vec<u8>, String> {
        let sdk = self.sdk()?;
        fs::create_dir_all(&self.cache_dir)
            .map_err(|e| format!("{}: {}", self.cache_dir.display(), e))?;
        let src = self.cache_dir.join(format!("{key}.rs"));
        let tmp = self.cache_dir.join(format!("{key}.wasm.tmp"));
        fs::write(&src, source).map_err(|e| format!("{}: {}", src.display(), e))?;

        let mut cmd = Command::new(&self.rustc);
        cmd.args(["--edition", "2024", "--crate-type", "cdylib", "--crate-name", "page"])
            .args(["--target", &self.target])
            // Fast to build: the module is small and runs once per load.
            .args(["-C", "opt-level=1", "-C", "debuginfo=0"])
            .args(["-C", "panic=abort", "-C", "codegen-units=16"])
            .args(["-A", "warnings", "-F", "unsafe_code"])
            .arg("-L")
            .arg(format!("dependency={}", sdk.join("deps").display()))
            .arg("-L")
            .arg(sdk)
            .arg("--extern")
            .arg(format!("cre={}", sdk.join("libcre.rlib").display()))
            .arg("-o")
            .arg(&tmp)
            .arg(&src);
        let out = cmd.output();
        let _ = fs::remove_file(&src);
        let out = out.map_err(|e| format!("cannot run {}: {}", self.rustc.display(), e))?;
        if !out.status.success() {
            let _ = fs::remove_file(&tmp);
            let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if msg.chars().count() > MAX_ERROR {
                msg = msg.chars().take(MAX_ERROR).collect::<String>() + "\n...";
            }
            return Err(if msg.is_empty() {
                "compiling failed".to_string()
            } else {
                msg
            });
        }
        let bytes = fs::read(&tmp).map_err(|e| format!("{}: {}", tmp.display(), e))?;
        if !is_wasm(&bytes) {
            let _ = fs::remove_file(&tmp);
            return Err("the compiler did not produce a wasm module".to_string());
        }
        fs::rename(&tmp, wasm).map_err(|e| format!("{}: {}", wasm.display(), e))?;
        Ok(bytes)
    }
}

/// FNV-1a over several byte strings (separated), 128 bits, as 32 hex digits. Not
/// cryptographic: it names cache files.
fn hash128(parts: &[&[u8]]) -> String {
    const PRIME: u128 = 0x0000000001000000000000000000013B;
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for part in parts {
        for &b in part.iter().chain(&[0xffu8]) {
            h ^= b as u128;
            h = h.wrapping_mul(PRIME);
        }
    }
    format!("{:032x}", h)
}

/// The `.wasm` that goes with a source URL: `a/demo.rs` -> `a/demo.wasm`.
pub fn wasm_sibling(url: &Url) -> Option<Url> {
    let name = url.path().rsplit('/').next().filter(|n| !n.is_empty())?;
    let stem = name.strip_suffix(".rs").unwrap_or(name);
    if stem.is_empty() || stem.ends_with(".wasm") {
        return None;
    }
    url.resolve(&format!("{stem}.wasm")).ok()
}

/// Turns fetched bytes into a module: wasm is used as it is, anything else is taken as
/// source and compiled. If that fails, the pre-compiled module beside `origin` is
/// requested; if that fails too the compile error is returned, with the reason.
pub fn module_from_bytes(
    bytes: Vec<u8>,
    origin: &Url,
    loader: &dyn Loader,
    compiler: &Compiler,
) -> Result<Vec<u8>, PluginError> {
    if is_wasm(&bytes) {
        return Ok(bytes);
    }
    let compile_err = match String::from_utf8(bytes) {
        Err(_) => "neither a wasm module nor UTF-8 source".to_string(),
        Ok(src) => match compiler.compile(&src) {
            Ok(m) => return Ok(m),
            Err(e) => e,
        },
    };
    let Some(alt) = wasm_sibling(origin) else {
        return Err(PluginError::Compile(compile_err));
    };
    match loader.load_bytes(&alt) {
        Ok(b) if is_wasm(&b) => Ok(b),
        Ok(_) => Err(PluginError::Compile(format!(
            "{compile_err}\n(the pre-compiled {alt} is not a wasm module)"
        ))),
        Err(e) => Err(PluginError::Compile(format!(
            "{compile_err}\n(could not get the pre-compiled {alt}: {e})"
        ))),
    }
}

/// Fetches `url` and returns its wasm module (see [`module_from_bytes`]).
pub fn load_module(
    url: &Url,
    loader: &dyn Loader,
    compiler: &Compiler,
) -> Result<Vec<u8>, PluginError> {
    let bytes = loader
        .load_bytes(url)
        .map_err(|e| PluginError::Fetch(format!("{}: {}", url, e)))?;
    module_from_bytes(bytes, url, loader, compiler)
}
