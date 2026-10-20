//! libmpv, loaded at run time (no link-time dependency, no headers needed).
//!
//! Only the small part of the client API Yuzu needs is bound: options/properties/commands,
//! the event loop and the software renderer (`MPV_RENDER_API_TYPE_SW`), which draws the
//! current video frame — with libass subtitles — into an RGBA buffer. When the library is
//! missing, [`Mpv::new`] fails and callers fall back to the ffmpeg command-line path.

use libloading::Library;
use std::ffi::{c_char, c_double, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub const FORMAT_NONE: c_int = 0;
pub const FORMAT_STRING: c_int = 1;
pub const FORMAT_FLAG: c_int = 3;
pub const FORMAT_INT64: c_int = 4;
pub const FORMAT_DOUBLE: c_int = 5;

const EVENT_SHUTDOWN: c_int = 1;
const EVENT_LOG_MESSAGE: c_int = 2;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_VIDEO_RECONFIG: c_int = 17;
const EVENT_SEEK: c_int = 20;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_SW_SIZE: c_int = 17;
const RENDER_PARAM_SW_FORMAT: c_int = 18;
const RENDER_PARAM_SW_STRIDE: c_int = 19;
const RENDER_PARAM_SW_POINTER: c_int = 20;
const RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawLog {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
}

type UpdateCb = unsafe extern "C" fn(*mut c_void);

struct Api {
    _lib: Library,
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    command: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    command_async: unsafe extern "C" fn(*mut c_void, u64, *mut *const c_char) -> c_int,
    observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    request_log_messages: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    wait_event: unsafe extern "C" fn(*mut c_void, c_double) -> *mut RawEvent,
    wakeup: unsafe extern "C" fn(*mut c_void),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    free: unsafe extern "C" fn(*mut c_void),
    rc_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    rc_set_update_callback: unsafe extern "C" fn(*mut c_void, Option<UpdateCb>, *mut c_void),
    rc_update: unsafe extern "C" fn(*mut c_void) -> u64,
    rc_render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    rc_free: unsafe extern "C" fn(*mut c_void),
}

fn candidates() -> Vec<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["libmpv-2.dll", "mpv-2.dll", "mpv-1.dll"]
    } else if cfg!(target_os = "macos") {
        &["libmpv.2.dylib", "libmpv.dylib"]
    } else {
        &["libmpv.so.2", "libmpv.so.1", "libmpv.so"]
    };
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("YUZU_LIBMPV") { v.push(PathBuf::from(d)); }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        for n in names { v.push(dir.join(n)); }
    }
    for n in names { v.push(PathBuf::from(n)); }
    if cfg!(target_os = "macos") {
        for d in ["/opt/homebrew/lib", "/usr/local/lib"] { for n in names { v.push(PathBuf::from(d).join(n)); } }
    }
    v
}

/// Resolve one symbol. `T` must be the exact C signature of `name` (NUL-terminated).
fn sym<T: Copy>(lib: &Library, name: &str) -> Result<T, String> {
    // SAFETY: callers pass the libmpv C signature for the symbol; the value is copied out.
    unsafe { lib.get::<T>(name.as_bytes()) }.map(|s| *s).map_err(|e| format!("{}: {}", name.trim_end_matches('\0'), e))
}

fn load_api() -> Result<Api, String> {
    let mut last = String::from("libmpv not found");
    for c in candidates() {
        // SAFETY: loading a shared library runs its initialisers; libmpv's are benign.
        let lib = match unsafe { Library::new(&c) } {
            Ok(l) => l,
            Err(e) => { last = format!("{}: {}", c.display(), e); continue; }
        };
        let api = Api {
            create: sym(&lib, "mpv_create\0")?,
            initialize: sym(&lib, "mpv_initialize\0")?,
            terminate_destroy: sym(&lib, "mpv_terminate_destroy\0")?,
            set_option_string: sym(&lib, "mpv_set_option_string\0")?,
            set_property_string: sym(&lib, "mpv_set_property_string\0")?,
            get_property: sym(&lib, "mpv_get_property\0")?,
            command: sym(&lib, "mpv_command\0")?,
            command_async: sym(&lib, "mpv_command_async\0")?,
            observe_property: sym(&lib, "mpv_observe_property\0")?,
            request_log_messages: sym(&lib, "mpv_request_log_messages\0")?,
            wait_event: sym(&lib, "mpv_wait_event\0")?,
            wakeup: sym(&lib, "mpv_wakeup\0")?,
            error_string: sym(&lib, "mpv_error_string\0")?,
            free: sym(&lib, "mpv_free\0")?,
            rc_create: sym(&lib, "mpv_render_context_create\0")?,
            rc_set_update_callback: sym(&lib, "mpv_render_context_set_update_callback\0")?,
            rc_update: sym(&lib, "mpv_render_context_update\0")?,
            rc_render: sym(&lib, "mpv_render_context_render\0")?,
            rc_free: sym(&lib, "mpv_render_context_free\0")?,
            _lib: lib,
        };
        return Ok(api);
    }
    Err(last)
}

static API: OnceLock<Result<Arc<Api>, String>> = OnceLock::new();

fn api() -> Result<Arc<Api>, String> {
    API.get_or_init(|| load_api().map(Arc::new)).clone()
}

/// True when libmpv can be loaded on this machine.
pub fn available() -> bool {
    api().is_ok()
}

/// Why libmpv could not be loaded (for the About / diagnostics screen).
pub fn load_error() -> Option<String> {
    api().err()
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    None,
    Flag(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Shutdown,
    Log { level: String, prefix: String, text: String },
    FileLoaded,
    /// reason: 0 eof, 2 stop, 3 quit, 4 error, 5 redirect
    EndFile { reason: i32, error: i32 },
    VideoReconfig,
    Seek,
    PlaybackRestart,
    Property { id: u64, name: String, value: PropValue },
    Other(i32),
}

/// An mpv core. The client API is thread-safe, so this can be shared between threads
/// (only one thread may call [`Mpv::wait_event`]).
pub struct Mpv {
    api: Arc<Api>,
    h: *mut c_void,
}

// SAFETY: libmpv's client API may be called from any thread (see client.h, "Thread safety").
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

impl Mpv {
    /// Create and initialise a core with the given options (`vo=libmpv` is added automatically).
    pub fn new(options: &[(&str, &str)]) -> Result<Mpv, String> {
        let api = api()?;
        // SAFETY: plain constructor call.
        let h = unsafe { (api.create)() };
        if h.is_null() { return Err("mpv_create failed".into()); }
        let m = Mpv { api, h };
        m.set_option("vo", "libmpv")?;
        for (k, v) in options { m.set_option(k, v)?; }
        // SAFETY: valid handle.
        let r = unsafe { (m.api.initialize)(m.h) };
        if r < 0 { return Err(format!("mpv_initialize: {}", m.err(r))); }
        Ok(m)
    }

    fn err(&self, code: c_int) -> String {
        // SAFETY: mpv_error_string returns a static string for any code.
        unsafe { CStr::from_ptr((self.api.error_string)(code)) }.to_string_lossy().into_owned()
    }

    pub fn set_option(&self, k: &str, v: &str) -> Result<(), String> {
        let (k2, v2) = (cstr(k), cstr(v));
        // SAFETY: valid handle and NUL-terminated strings.
        let r = unsafe { (self.api.set_option_string)(self.h, k2.as_ptr(), v2.as_ptr()) };
        if r < 0 { Err(format!("option {}={}: {}", k, v, self.err(r))) } else { Ok(()) }
    }

    pub fn set_property(&self, k: &str, v: &str) -> Result<(), String> {
        let (k2, v2) = (cstr(k), cstr(v));
        // SAFETY: valid handle and NUL-terminated strings.
        let r = unsafe { (self.api.set_property_string)(self.h, k2.as_ptr(), v2.as_ptr()) };
        if r < 0 { Err(format!("property {}={}: {}", k, v, self.err(r))) } else { Ok(()) }
    }

    fn args(args: &[&str]) -> (Vec<CString>, Vec<*const c_char>) {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        (owned, ptrs)
    }

    /// Run a command synchronously (`["seek", "1.5", "absolute+exact"]`).
    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        let (_owned, mut ptrs) = Self::args(args);
        // SAFETY: NULL-terminated array of valid C strings that outlive the call.
        let r = unsafe { (self.api.command)(self.h, ptrs.as_mut_ptr()) };
        if r < 0 { Err(format!("{}: {}", args.join(" "), self.err(r))) } else { Ok(()) }
    }

    /// Queue a command without waiting for it (seeks while scrubbing).
    pub fn command_async(&self, args: &[&str]) {
        let (_owned, mut ptrs) = Self::args(args);
        // SAFETY: mpv copies the arguments before returning.
        unsafe { (self.api.command_async)(self.h, 0, ptrs.as_mut_ptr()) };
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let n = cstr(name);
        let mut v: f64 = 0.0;
        // SAFETY: FORMAT_DOUBLE writes a double into the pointed-to value.
        let r = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_DOUBLE, &mut v as *mut f64 as *mut c_void) };
        (r >= 0).then_some(v)
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        let n = cstr(name);
        let mut v: i64 = 0;
        // SAFETY: FORMAT_INT64 writes an int64 into the pointed-to value.
        let r = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_INT64, &mut v as *mut i64 as *mut c_void) };
        (r >= 0).then_some(v)
    }

    pub fn get_flag(&self, name: &str) -> Option<bool> {
        let n = cstr(name);
        let mut v: c_int = 0;
        // SAFETY: FORMAT_FLAG writes an int into the pointed-to value.
        let r = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_FLAG, &mut v as *mut c_int as *mut c_void) };
        (r >= 0).then_some(v != 0)
    }

    pub fn get_string(&self, name: &str) -> Option<String> {
        let n = cstr(name);
        let mut p: *mut c_char = std::ptr::null_mut();
        // SAFETY: FORMAT_STRING stores a newly allocated string that we free with mpv_free.
        let r = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_STRING, &mut p as *mut *mut c_char as *mut c_void) };
        if r < 0 || p.is_null() { return None; }
        // SAFETY: p is a valid NUL-terminated string owned by mpv.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        // SAFETY: allocated by mpv.
        unsafe { (self.api.free)(p as *mut c_void) };
        Some(s)
    }

    /// Ask for `Event::Property` notifications of `name` (format: FORMAT_DOUBLE, FORMAT_FLAG, …).
    pub fn observe(&self, id: u64, name: &str, format: c_int) -> Result<(), String> {
        let n = cstr(name);
        // SAFETY: valid handle and string.
        let r = unsafe { (self.api.observe_property)(self.h, id, n.as_ptr(), format) };
        if r < 0 { Err(format!("observe {}: {}", name, self.err(r))) } else { Ok(()) }
    }

    pub fn request_log(&self, level: &str) {
        let l = cstr(level);
        // SAFETY: valid handle and string.
        unsafe { (self.api.request_log_messages)(self.h, l.as_ptr()) };
    }

    /// Interrupt a blocking [`Mpv::wait_event`].
    pub fn wakeup(&self) {
        // SAFETY: valid handle.
        unsafe { (self.api.wakeup)(self.h) };
    }

    /// Wait up to `timeout` seconds for the next event (None on timeout).
    pub fn wait_event(&self, timeout: f64) -> Option<Event> {
        // SAFETY: the returned event stays valid until the next wait_event call on this handle.
        let e = unsafe { &*(self.api.wait_event)(self.h, timeout) };
        let s = |p: *const c_char| if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() };
        Some(match e.event_id {
            0 => return None,
            EVENT_SHUTDOWN => Event::Shutdown,
            EVENT_LOG_MESSAGE => {
                // SAFETY: data points to mpv_event_log_message for this event id.
                let l = unsafe { &*(e.data as *const RawLog) };
                Event::Log { level: s(l.level), prefix: s(l.prefix), text: s(l.text) }
            }
            EVENT_FILE_LOADED => Event::FileLoaded,
            EVENT_END_FILE => {
                // SAFETY: data points to mpv_event_end_file for this event id.
                let r = unsafe { &*(e.data as *const RawEndFile) };
                Event::EndFile { reason: r.reason, error: r.error }
            }
            EVENT_VIDEO_RECONFIG => Event::VideoReconfig,
            EVENT_SEEK => Event::Seek,
            EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
            EVENT_PROPERTY_CHANGE => {
                // SAFETY: data points to mpv_event_property; its payload matches `format`.
                let p = unsafe { &*(e.data as *const RawProperty) };
                let value = unsafe {
                    match p.format {
                        FORMAT_FLAG if !p.data.is_null() => PropValue::Flag(*(p.data as *const c_int) != 0),
                        FORMAT_INT64 if !p.data.is_null() => PropValue::Int(*(p.data as *const i64)),
                        FORMAT_DOUBLE if !p.data.is_null() => PropValue::Double(*(p.data as *const f64)),
                        FORMAT_STRING if !p.data.is_null() => PropValue::Str(s(*(p.data as *const *const c_char))),
                        _ => PropValue::None,
                    }
                };
                Event::Property { id: e.reply_userdata, name: s(p.name), value }
            }
            other => Event::Other(other),
        })
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: valid handle; every SwRenderer holding an Arc<Mpv> has been dropped already.
        unsafe { (self.api.terminate_destroy)(self.h) };
    }
}

struct CbBox(Box<dyn Fn() + Send + Sync>);

unsafe extern "C" fn update_trampoline(ctx: *mut c_void) {
    // SAFETY: ctx is the CbBox owned by the SwRenderer, alive until the context is freed.
    let cb = unsafe { &*(ctx as *const CbBox) };
    (cb.0)();
}

/// A 64-byte aligned RGBA surface (mpv wants aligned pointers and strides for its SIMD paths).
pub struct Surface {
    buf: Vec<u8>,
    off: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Surface {
        let stride = (width * 4).div_ceil(64) * 64;
        let buf = vec![0u8; stride * height.max(1) + 64];
        let off = (64 - (buf.as_ptr() as usize % 64)) % 64;
        Surface { buf, off, width, height, stride }
    }
    pub fn data(&self) -> &[u8] {
        &self.buf[self.off..self.off + self.stride * self.height]
    }
    fn ptr(&mut self) -> *mut c_void {
        self.buf[self.off..].as_mut_ptr() as *mut c_void
    }
    /// Tightly packed RGBA copy with opaque alpha (mpv leaves the 4th byte undefined).
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        let d = self.data();
        for y in 0..self.height {
            let row = &d[y * self.stride..y * self.stride + self.width * 4];
            out.extend_from_slice(row);
        }
        for a in out.iter_mut().skip(3).step_by(4) { *a = 255; }
        out
    }
}

/// mpv's software renderer. Must be used from a single thread; drop it before the core.
pub struct SwRenderer {
    mpv: Arc<Mpv>,
    ctx: *mut c_void,
    _cb: Box<CbBox>,
}

// SAFETY: the render context is only ever used by the thread that owns the SwRenderer.
unsafe impl Send for SwRenderer {}

impl SwRenderer {
    /// `on_update` is called from an mpv thread whenever a new frame (or a redraw) is available;
    /// it must only signal (e.g. a condvar), never call back into mpv.
    pub fn new(mpv: Arc<Mpv>, on_update: Box<dyn Fn() + Send + Sync>) -> Result<SwRenderer, String> {
        let api_type = cstr("sw");
        let mut params = [
            RenderParam { kind: RENDER_PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
            RenderParam { kind: 0, data: std::ptr::null_mut() },
        ];
        let mut ctx: *mut c_void = std::ptr::null_mut();
        // SAFETY: valid core handle and a 0-terminated parameter list.
        let r = unsafe { (mpv.api.rc_create)(&mut ctx, mpv.h, params.as_mut_ptr()) };
        if r < 0 { return Err(format!("mpv_render_context_create: {}", mpv.err(r))); }
        let cb = Box::new(CbBox(on_update));
        // SAFETY: cb lives as long as the context (freed in Drop before cb).
        unsafe { (mpv.api.rc_set_update_callback)(ctx, Some(update_trampoline), &*cb as *const CbBox as *mut c_void) };
        Ok(SwRenderer { mpv, ctx, _cb: cb })
    }

    /// True when a new frame should be rendered (call after the update callback fired).
    pub fn update(&self) -> bool {
        // SAFETY: valid context.
        unsafe { (self.mpv.api.rc_update)(self.ctx) & RENDER_UPDATE_FRAME != 0 }
    }

    /// Render the current frame into `s` (letterboxed to its size). Blocks until the frame's
    /// display time during playback, which keeps video in step with audio.
    pub fn render(&self, s: &mut Surface) -> Result<(), String> {
        let mut size: [c_int; 2] = [s.width as c_int, s.height as c_int];
        let fmt = cstr("rgb0");
        let mut stride: usize = s.stride;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_SW_SIZE, data: size.as_mut_ptr() as *mut c_void },
            RenderParam { kind: RENDER_PARAM_SW_FORMAT, data: fmt.as_ptr() as *mut c_void },
            RenderParam { kind: RENDER_PARAM_SW_STRIDE, data: &mut stride as *mut usize as *mut c_void },
            RenderParam { kind: RENDER_PARAM_SW_POINTER, data: s.ptr() },
            RenderParam { kind: 0, data: std::ptr::null_mut() },
        ];
        // SAFETY: parameters point to live, correctly sized data for the duration of the call.
        let r = unsafe { (self.mpv.api.rc_render)(self.ctx, params.as_mut_ptr()) };
        if r < 0 { Err(format!("render: {}", self.mpv.err(r))) } else { Ok(()) }
    }
}

impl Drop for SwRenderer {
    fn drop(&mut self) {
        // SAFETY: valid context; after this no callbacks are made.
        unsafe {
            (self.mpv.api.rc_set_update_callback)(self.ctx, None, std::ptr::null_mut());
            (self.mpv.api.rc_free)(self.ctx);
        }
    }
}
