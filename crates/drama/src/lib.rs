//! Крейт drama — FFI-обвязка над drama_engine.dll (Fish Audio Drama 3, форк timoncool/drama-rs, C-ABI
//! `capi/drama_engine.h`). Один движок обслуживает одного вызывающего; `cancel` — из любого потока.

use libloading::Library;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use thiserror::Error;

const DRAMA_OK: i32 = 0;
const DRAMA_CANCELLED: i32 = 2;

#[derive(Debug, Error)]
pub enum DramaError {
    #[error("drama_engine: {0}")]
    Load(String),
    #[error("drama_engine: {0}")]
    Engine(String),
    #[error("drama_engine: generation cancelled")]
    Cancelled,
}

/// Параметры сэмплинга одной реплики. `max_new` — потолок кадров каждого предложения (21.5 кадра на секунду речи).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub temperature: f32,
    pub top_k: i32,
    pub top_p: f32,
    pub seed: u64,
    pub max_new: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self { temperature: 1.0, top_k: 30, top_p: 0.9, seed: 0, max_new: 2048 }
    }
}

/// Кадров в секунду речи у кодека Drama 3.
pub const FRAMES_PER_SECOND: f64 = 21.5;

#[repr(C)]
struct RawEngine {
    _private: [u8; 0],
}

type FnVersion = unsafe extern "C" fn() -> *const c_char;
type FnSampleRate = unsafe extern "C" fn() -> u32;
type FnLastError = unsafe extern "C" fn() -> *const c_char;
type FnLoad = unsafe extern "C" fn(*const c_char, u32, u32) -> *mut RawEngine;
type FnFree = unsafe extern "C" fn(*mut RawEngine);
type FnSpeak = unsafe extern "C" fn(*mut RawEngine, *const c_char, *const c_char, *const c_char, *const Params, *mut *mut f32, *mut usize) -> i32;
type FnChunk = unsafe extern "C" fn(*const f32, usize, *mut c_void);
type FnSpeakStream = unsafe extern "C" fn(
    *mut RawEngine,
    *const c_char,
    *const c_char,
    *const c_char,
    *const Params,
    Option<FnChunk>,
    *mut c_void,
    *mut *mut f32,
    *mut usize,
) -> i32;
type FnFreeAudio = unsafe extern "C" fn(*mut f32, usize);
type FnCancel = unsafe extern "C" fn(*mut RawEngine);

struct Api {
    version: FnVersion,
    sample_rate: FnSampleRate,
    last_error: FnLastError,
    load: FnLoad,
    free: FnFree,
    speak: FnSpeak,
    /// Нет у DLL до engine-0.1.2.
    speak_stream: Option<FnSpeakStream>,
    free_audio: FnFreeAudio,
    cancel: FnCancel,
}

unsafe extern "C" fn chunk_trampoline(audio: *const f32, len: usize, user: *mut c_void) {
    if audio.is_null() || user.is_null() {
        return;
    }
    let sink = &*(user as *const &(dyn Fn(&[f32]) + Sync));
    sink(std::slice::from_raw_parts(audio, len));
}

/// Загруженные веса Drama 3 на видеокарте; освобождаются при drop.
pub struct DramaEngine {
    api: Api,
    raw: *mut RawEngine,
    _lib: Library,
}

// Движок сериализует вызовы speak сам (mutex внутри DLL); cancel потокобезопасен.
unsafe impl Send for DramaEngine {}
unsafe impl Sync for DramaEngine {}

fn cstring(s: &str, what: &str) -> Result<CString, DramaError> {
    CString::new(s).map_err(|_| DramaError::Engine(format!("{what} contains NUL")))
}

impl DramaEngine {
    /// Грузит DLL и веса из `model_dir`. `lib_dirs` — каталоги её зависимостей (cuBLAS), они идут в PATH.
    pub fn load(dll: &Path, lib_dirs: &[&Path], model_dir: &Path, max_len: u32) -> Result<Self, DramaError> {
        for dir in lib_dirs.iter().copied().chain(dll.parent()) {
            add_dll_directory(dir);
        }
        let lib = unsafe { Library::new(dll) }.map_err(|e| DramaError::Load(format!("{}: {e}", dll.display())))?;
        let api = unsafe {
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| DramaError::Load(format!("{}: {e}", $name)))?
                };
            }
            Api {
                version: sym!("drama_version"),
                sample_rate: sym!("drama_sample_rate"),
                last_error: sym!("drama_last_error"),
                load: sym!("drama_load"),
                free: sym!("drama_free"),
                speak: sym!("drama_speak"),
                speak_stream: lib.get::<FnSpeakStream>(b"drama_speak_stream\0").ok().map(|s| *s),
                free_audio: sym!("drama_free_audio"),
                cancel: sym!("drama_cancel"),
            }
        };
        let dir = cstring(&model_dir.to_string_lossy(), "model_dir")?;
        let raw = unsafe { (api.load)(dir.as_ptr(), max_len, 512) };
        if raw.is_null() {
            return Err(DramaError::Engine(unsafe { last_error(&api) }));
        }
        Ok(Self { api, raw, _lib: lib })
    }

    pub fn version(&self) -> String {
        unsafe { CStr::from_ptr((self.api.version)()) }.to_string_lossy().into_owned()
    }

    pub fn sample_rate(&self) -> u32 {
        unsafe { (self.api.sample_rate)() }
    }

    /// Озвучить `text` голосом рефа `ref_wav` (его точный транскрипт — `ref_text`). Моно f32 на `sample_rate()`.
    pub fn speak(&self, text: &str, ref_wav: &Path, ref_text: &str, params: &Params) -> Result<Vec<f32>, DramaError> {
        let (t, w, r) = (cstring(text, "text")?, cstring(&ref_wav.to_string_lossy(), "ref_wav")?, cstring(ref_text, "ref_text")?);
        let mut ptr: *mut f32 = std::ptr::null_mut();
        let mut len = 0usize;
        let code = unsafe { (self.api.speak)(self.raw, t.as_ptr(), w.as_ptr(), r.as_ptr(), params, &mut ptr, &mut len) };
        self.take_audio(code, ptr, len)
    }

    /// Отдаёт ли DLL звук кусками по мере синтеза (`speak_stream`).
    pub fn supports_streaming(&self) -> bool {
        self.api.speak_stream.is_some()
    }

    /// Как `speak`, и `on_chunk` получает каждый декодированный кусок (предложение и больше) из потока кодека DLL.
    /// У DLL без потоковой точки — ошибка, без молчаливого перехода на `speak`.
    pub fn speak_stream(
        &self,
        text: &str,
        ref_wav: &Path,
        ref_text: &str,
        params: &Params,
        on_chunk: &(dyn Fn(&[f32]) + Sync),
    ) -> Result<Vec<f32>, DramaError> {
        let stream = self.api.speak_stream.ok_or_else(|| DramaError::Engine("drama_speak_stream is missing in this DLL".into()))?;
        let (t, w, r) = (cstring(text, "text")?, cstring(&ref_wav.to_string_lossy(), "ref_wav")?, cstring(ref_text, "ref_text")?);
        let mut ptr: *mut f32 = std::ptr::null_mut();
        let mut len = 0usize;
        let user = &on_chunk as *const &(dyn Fn(&[f32]) + Sync) as *mut c_void;
        let code = unsafe {
            stream(self.raw, t.as_ptr(), w.as_ptr(), r.as_ptr(), params, Some(chunk_trampoline), user, &mut ptr, &mut len)
        };
        self.take_audio(code, ptr, len)
    }

    fn take_audio(&self, code: i32, ptr: *mut f32, len: usize) -> Result<Vec<f32>, DramaError> {
        match code {
            DRAMA_OK => {
                let audio = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
                unsafe { (self.api.free_audio)(ptr, len) };
                Ok(audio)
            }
            DRAMA_CANCELLED => Err(DramaError::Cancelled),
            _ => Err(DramaError::Engine(unsafe { last_error(&self.api) })),
        }
    }

    /// Остановить идущую генерацию на следующем кадре (speak вернёт Cancelled).
    pub fn cancel(&self) {
        unsafe { (self.api.cancel)(self.raw) }
    }
}

impl Drop for DramaEngine {
    fn drop(&mut self) {
        unsafe { (self.api.free)(self.raw) }
    }
}

unsafe fn last_error(api: &Api) -> String {
    let p = (api.last_error)();
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

#[cfg(target_os = "windows")]
fn add_dll_directory(path: &Path) {
    let dir = path.to_string_lossy().into_owned();
    let current = std::env::var("PATH").unwrap_or_default();
    if !current.split(';').any(|p| Path::new(p) == path) {
        std::env::set_var("PATH", if current.is_empty() { dir } else { format!("{dir};{current}") });
    }
}

#[cfg(not(target_os = "windows"))]
fn add_dll_directory(_path: &Path) {}
