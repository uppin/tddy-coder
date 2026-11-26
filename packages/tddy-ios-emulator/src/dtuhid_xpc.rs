//! The production input port on macOS: touch and keyboard events sent to the simulator's `dtuhidd`
//! over XPC, the way Xcode 27's own tools do.
//!
//! Ported from idb (<https://github.com/meta/idb>), MIT licence, Copyright (c) Meta Platforms, Inc.
//! and affiliates — see `THIRD_PARTY_NOTICES` in this crate. The connection follows
//! `FBSimulatorControl/XPC/SimulatorXPCConnection.swift` (look the service's Mach port up in the
//! device's bootstrap namespace with `-[SimDevice lookup:error:]`, wrap it with the private
//! `xpc_endpoint_create_mach_port_4sim`, mark the connection `xpc_connection_enable_sim2host_4sim`)
//! and `FBSimulatorControl/HID/SimulatorDTUHIDConnection.swift` (a barrier round trip proves a live
//! daemon before any event is sent, retried while launchd throttles a daemon that aborted during
//! boot).
//!
//! CoreSimulator is a private framework, so it is loaded at run time and messaged through the
//! Objective-C runtime; nothing links against it. libxpc is public: only the calls made here are
//! declared.

use std::ffi::{c_char, c_void, CStr, CString};
use std::process::Command;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context as _};
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSError, NSString};

use crate::dtuhid::{
    home_button, keyboard, keystrokes_for_char, liveness_barrier, screen_ratio, touch,
    usage_for_special, ButtonState, ContactTracker, WireValue, DIGITIZER_SERVICE,
};
use crate::geometry::Rect;
use crate::input::{InputEvent, InputSink};

/// CoreSimulator, which knows the devices and vends their guest services.
const CORE_SIMULATOR: &CStr =
    c"/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator";

/// How long the liveness barrier is waited on, how often it is tried, and how long between tries
/// (idb's `DTUHIDTiming`): the first send demand-launches `dtuhidd`, and launchd throttles the
/// respawn of one that aborted early in boot.
const LIVENESS_TIMEOUT: Duration = Duration::from_secs(4);
const LIVENESS_ATTEMPTS: u32 = 5;
const LIVENESS_BACKOFF: Duration = Duration::from_secs(4);
/// The device-open tail after the barrier reply, before the first event.
const REPLY_TAIL: Duration = Duration::from_millis(200);

type XpcObject = *mut c_void;

extern "C" {
    fn xpc_dictionary_create(
        keys: *const *const c_char,
        values: *const XpcObject,
        count: usize,
    ) -> XpcObject;
    fn xpc_dictionary_set_string(dictionary: XpcObject, key: *const c_char, value: *const c_char);
    fn xpc_dictionary_set_bool(dictionary: XpcObject, key: *const c_char, value: bool);
    fn xpc_dictionary_set_double(dictionary: XpcObject, key: *const c_char, value: f64);
    fn xpc_dictionary_set_uint64(dictionary: XpcObject, key: *const c_char, value: u64);
    fn xpc_dictionary_set_value(dictionary: XpcObject, key: *const c_char, value: XpcObject);
    fn xpc_get_type(object: XpcObject) -> *const c_void;
    fn xpc_release(object: XpcObject);
    fn xpc_connection_set_event_handler(
        connection: XpcObject,
        handler: &block2::Block<dyn Fn(XpcObject)>,
    );
    fn xpc_connection_resume(connection: XpcObject);
    fn xpc_connection_send_message(connection: XpcObject, message: XpcObject);
    fn xpc_connection_send_message_with_reply_sync(
        connection: XpcObject,
        message: XpcObject,
    ) -> XpcObject;
    fn xpc_connection_cancel(connection: XpcObject);
    static _xpc_type_error: c_void;
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

const RTLD_NOW: i32 = 0x2;

type EndpointFromPort = unsafe extern "C" fn(u32, u64, u64) -> XpcObject;
type ConnectionFromEndpoint = unsafe extern "C" fn(XpcObject) -> XpcObject;
type EnableSim2Host = unsafe extern "C" fn(XpcObject);

/// Touch and keyboard input for one booted simulator, through its `dtuhidd`.
///
/// Touch arrives as screen points inside `device_area` (the client maps device pixels there); it is
/// sent as the point's position on the device screen, so it does not depend on where the window is.
pub struct DtuhidInput {
    connection: DtuhidConnection,
    device_area: Rect,
    contact: Mutex<ContactTracker>,
}

impl DtuhidInput {
    /// Connect to the `dtuhidd` of the booted device `udid`, whose screen the viewer sees at
    /// `device_area`.
    pub fn connect(udid: &str, device_area: Rect) -> anyhow::Result<Self> {
        Ok(Self {
            connection: DtuhidConnection::connect(udid)?,
            device_area,
            contact: Mutex::new(ContactTracker::default()),
        })
    }

    fn contact(&self) -> std::sync::MutexGuard<'_, ContactTracker> {
        self.contact
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Press and release the Home button.
    pub fn press_home(&self) -> anyhow::Result<()> {
        self.connection.send(&home_button(ButtonState::Down))?;
        self.connection.send(&home_button(ButtonState::Up))
    }
}

impl InputSink for DtuhidInput {
    fn post(&self, event: InputEvent) -> anyhow::Result<()> {
        let at = |point| screen_ratio(point, self.device_area);
        match event {
            // A touch screen has no hover.
            InputEvent::MouseMove(_) => Ok(()),
            InputEvent::MouseDown(point) | InputEvent::MouseDrag(point) => {
                let (x, y) = at(point);
                let phase = self.contact().down();
                self.connection.send(&touch(x, y, phase))
            }
            InputEvent::MouseUp(point) => {
                let (x, y) = at(point);
                let phase = self.contact().up();
                self.connection.send(&touch(x, y, phase))
            }
            InputEvent::Text(text) => {
                for c in text.chars() {
                    let strokes = keystrokes_for_char(c);
                    if strokes.is_empty() {
                        log::debug!("ios-emulator: no US keyboard usage types {c:?}; dropped");
                    }
                    for stroke in strokes {
                        self.connection.send(&stroke)?;
                    }
                }
                Ok(())
            }
            InputEvent::Key { key, pressed } => {
                let state = if pressed {
                    ButtonState::Down
                } else {
                    ButtonState::Up
                };
                self.connection
                    .send(&keyboard(usage_for_special(key), state))
            }
        }
    }
}

/// One host XPC connection to a device's `dtuhidd` digitizer service.
struct DtuhidConnection {
    connection: XpcObject,
    // Kept alive for as long as the connection: XPC holds it, but does not own the Rust side.
    _events: RcBlock<dyn Fn(XpcObject)>,
}

// SAFETY: XPC connections may be sent on and cancelled from any thread; the event handler block is
// only kept alive here, never called from Rust.
unsafe impl Send for DtuhidConnection {}
// SAFETY: as above — `&self` only sends on the connection.
unsafe impl Sync for DtuhidConnection {}

impl DtuhidConnection {
    fn connect(udid: &str) -> anyhow::Result<Self> {
        let mut last_failure = None;
        for attempt in 1..=LIVENESS_ATTEMPTS {
            let connection = Self::open(udid)?;
            match connection.confirm_liveness() {
                Ok(()) => {
                    std::thread::sleep(REPLY_TAIL);
                    return Ok(connection);
                }
                Err(error) => {
                    log::debug!(
                        "ios-emulator: dtuhidd did not answer (attempt {attempt} of \
                         {LIVENESS_ATTEMPTS}): {error:#}"
                    );
                    last_failure = Some(error);
                    if attempt < LIVENESS_ATTEMPTS {
                        std::thread::sleep(LIVENESS_BACKOFF);
                    }
                }
            }
        }
        Err(last_failure
            .unwrap_or_else(|| anyhow::anyhow!("no liveness attempt was made"))
            .context(format!(
                "the simulator's input daemon (dtuhidd, {DIGITIZER_SERVICE}) did not answer \
                 {LIVENESS_ATTEMPTS} liveness probes"
            )))
    }

    /// Look the service up and connect to it, unresumed until the event handler is set.
    fn open(udid: &str) -> anyhow::Result<Self> {
        let port = digitizer_port(udid)?;
        let sim = Sim4Symbols::resolve()?;
        // SAFETY: `port` is a send right just looked up; the endpoint consumes it. Both create
        // functions return +1 objects, released when the connection is dropped (the endpoint
        // right after the connection is made from it).
        let connection = unsafe {
            let endpoint = (sim.endpoint_from_port)(port, 0, 0);
            if endpoint.is_null() {
                bail!("could not make an XPC endpoint from {DIGITIZER_SERVICE}'s port");
            }
            let connection = (sim.connection_from_endpoint)(endpoint);
            xpc_release(endpoint);
            if connection.is_null() {
                bail!("could not connect to {DIGITIZER_SERVICE}");
            }
            (sim.enable_sim2host)(connection);
            connection
        };
        let events = RcBlock::new(|event: XpcObject| {
            // SAFETY: XPC hands the handler a valid object for the duration of the call.
            if std::ptr::eq(unsafe { xpc_get_type(event) }, unsafe { &_xpc_type_error }) {
                log::debug!("ios-emulator: dtuhidd connection reported an error event");
            }
        });
        // SAFETY: a fresh connection, given its handler before it is resumed.
        unsafe {
            xpc_connection_set_event_handler(connection, &events);
            xpc_connection_resume(connection);
        }
        Ok(Self {
            connection,
            _events: events,
        })
    }

    /// Round-trip the inert barrier: the only proof a daemon is behind the port, since a send to a
    /// throttled one reports no error.
    fn confirm_liveness(&self) -> anyhow::Result<()> {
        let probe = SendableXpc(xpc_object_of(&liveness_barrier()));
        let connection = SendableXpc(self.connection);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let (connection, probe) = (connection, probe);
            // SAFETY: both objects are live; the reply is a +1 object released here.
            let is_error = unsafe {
                let reply = xpc_connection_send_message_with_reply_sync(connection.0, probe.0);
                xpc_release(probe.0);
                let is_error = std::ptr::eq(xpc_get_type(reply), &_xpc_type_error);
                xpc_release(reply);
                is_error
            };
            let _ = sender.send(is_error);
        });
        match receiver.recv_timeout(LIVENESS_TIMEOUT) {
            Ok(false) => Ok(()),
            Ok(true) => bail!("XPC answered the liveness probe with an error: no daemon took it"),
            Err(_) => bail!("no reply to the liveness probe within {LIVENESS_TIMEOUT:?}"),
        }
    }

    fn send(&self, message: &WireValue) -> anyhow::Result<()> {
        let object = xpc_object_of(message);
        // SAFETY: a live connection and a fresh +1 message, released after the send has copied it.
        unsafe {
            xpc_connection_send_message(self.connection, object);
            xpc_release(object);
        }
        Ok(())
    }
}

impl Drop for DtuhidConnection {
    fn drop(&mut self) {
        // SAFETY: cancelling a resumed connection and releasing our reference to it.
        unsafe {
            xpc_connection_cancel(self.connection);
            xpc_release(self.connection);
        }
    }
}

/// An XPC object handed to the liveness thread, which owns its reference there.
struct SendableXpc(XpcObject);
// SAFETY: XPC objects are reference counted and safe to use from any thread.
unsafe impl Send for SendableXpc {}

/// The private `_4sim` libxpc symbols that make a simulator service's port a host connection.
struct Sim4Symbols {
    endpoint_from_port: EndpointFromPort,
    connection_from_endpoint: ConnectionFromEndpoint,
    enable_sim2host: EnableSim2Host,
}

impl Sim4Symbols {
    fn resolve() -> anyhow::Result<Self> {
        // SAFETY: `dlopen(NULL)` is this process's own symbol table.
        let handle = unsafe { dlopen(std::ptr::null(), RTLD_NOW) };
        let symbol = |name: &CStr| -> anyhow::Result<*mut c_void> {
            // SAFETY: looking a NUL-terminated name up in a live handle.
            let found = unsafe { dlsym(handle, name.as_ptr()) };
            if found.is_null() {
                bail!(
                    "libxpc has no {} in this macOS: a simulator's input service cannot be reached",
                    name.to_string_lossy()
                );
            }
            Ok(found)
        };
        // SAFETY: each symbol is the libxpc function of exactly this signature (idb's
        // `SimulatorXPCConnection`).
        unsafe {
            Ok(Self {
                endpoint_from_port: std::mem::transmute::<*mut c_void, EndpointFromPort>(symbol(
                    c"xpc_endpoint_create_mach_port_4sim",
                )?),
                connection_from_endpoint: std::mem::transmute::<*mut c_void, ConnectionFromEndpoint>(
                    symbol(c"xpc_connection_create_from_endpoint")?,
                ),
                enable_sim2host: std::mem::transmute::<*mut c_void, EnableSim2Host>(symbol(
                    c"xpc_connection_enable_sim2host_4sim",
                )?),
            })
        }
    }
}

/// The Mach port of the digitizer service in device `udid`'s bootstrap namespace.
fn digitizer_port(udid: &str) -> anyhow::Result<u32> {
    let device = sim_device(udid)?;
    let service = NSString::from_str(DIGITIZER_SERVICE);
    let mut error: *mut NSError = std::ptr::null_mut();
    // SAFETY: `lookup:error:` on a live `SimDevice` returns a `mach_port_t` and may set `error`.
    let port: u32 = unsafe { msg_send![&*device, lookup: &*service, error: &mut error] };
    if port == 0 {
        bail!(
            "the simulator {udid} vends no {DIGITIZER_SERVICE}: {}",
            describe(error)
        );
    }
    Ok(port)
}

/// CoreSimulator's `SimDevice` for `udid`, through the active Xcode's service context.
fn sim_device(udid: &str) -> anyhow::Result<Retained<AnyObject>> {
    let developer_dir = developer_dir()?;
    // SAFETY: loading a framework by absolute path; the result is checked.
    if unsafe { dlopen(CORE_SIMULATOR.as_ptr(), RTLD_NOW) }.is_null() {
        // SAFETY: `dlerror` returns the last error as a C string, or null.
        let reason = unsafe { dlerror() };
        let reason = if reason.is_null() {
            String::new()
        } else {
            // SAFETY: a non-null `dlerror` result is a NUL-terminated string.
            unsafe { CStr::from_ptr(reason) }
                .to_string_lossy()
                .into_owned()
        };
        bail!(
            "could not load CoreSimulator ({}): {reason}",
            CORE_SIMULATOR.to_string_lossy()
        );
    }
    let context_class = objc2::runtime::AnyClass::get(c"SimServiceContext")
        .context("CoreSimulator has no SimServiceContext class")?;
    let developer_dir = NSString::from_str(&developer_dir);
    let mut error: *mut NSError = std::ptr::null_mut();
    // SAFETY: CoreSimulator's class methods, each returning an object or nil and setting `error`.
    let context: Option<Retained<AnyObject>> = unsafe {
        msg_send![context_class, sharedServiceContextForDeveloperDir: &*developer_dir, error: &mut error]
    };
    let context = context
        .with_context(|| format!("CoreSimulator gave no service context: {}", describe(error)))?;
    let mut error: *mut NSError = std::ptr::null_mut();
    // SAFETY: as above.
    let set: Option<Retained<AnyObject>> =
        unsafe { msg_send![&*context, defaultDeviceSetWithError: &mut error] };
    let set =
        set.with_context(|| format!("CoreSimulator gave no device set: {}", describe(error)))?;
    // SAFETY: `devices` is an `NSArray` of `SimDevice`.
    let devices: Retained<NSArray<AnyObject>> = unsafe { msg_send![&*set, devices] };
    devices
        .iter()
        .find(|device| {
            // SAFETY: `UDID` is an `NSUUID`, `UUIDString` its canonical form.
            let uuid: Retained<AnyObject> = unsafe { msg_send![&**device, UDID] };
            let text: Retained<NSString> = unsafe { msg_send![&*uuid, UUIDString] };
            text.to_string().eq_ignore_ascii_case(udid)
        })
        .with_context(|| format!("CoreSimulator knows no device {udid}"))
}

/// The active Xcode's developer directory, as `xcode-select` reports it outside any developer
/// environment (the same cleaned environment `XcrunSimctl` runs in).
fn developer_dir() -> anyhow::Result<String> {
    let output = Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .env_remove("DEVELOPER_DIR")
        .env_remove("SDKROOT")
        .output()
        .context("running /usr/bin/xcode-select -p")?;
    if !output.status.success() {
        bail!(
            "/usr/bin/xcode-select -p failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn describe(error: *mut NSError) -> String {
    // SAFETY: a non-null error set by a CoreSimulator call is a live `NSError`.
    unsafe { error.as_ref() }
        .map(|error| error.localizedDescription().to_string())
        .unwrap_or_else(|| "no reason given".to_owned())
}

/// A +1 XPC object holding `value`.
fn xpc_object_of(value: &WireValue) -> XpcObject {
    // SAFETY: an empty dictionary from libxpc, filled below.
    let dictionary = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
    if let WireValue::Dictionary(entries) = value {
        for (key, entry) in entries {
            let key = CString::new(*key).expect("wire keys are plain identifiers");
            // SAFETY: setting values on a live dictionary; nested values are released once set,
            // since the dictionary retains them.
            unsafe {
                match entry {
                    WireValue::String(text) => {
                        let text = CString::new(text.as_str()).expect("wire strings carry no NUL");
                        xpc_dictionary_set_string(dictionary, key.as_ptr(), text.as_ptr());
                    }
                    WireValue::Bool(flag) => {
                        xpc_dictionary_set_bool(dictionary, key.as_ptr(), *flag)
                    }
                    WireValue::Double(number) => {
                        xpc_dictionary_set_double(dictionary, key.as_ptr(), *number)
                    }
                    WireValue::Uint64(number) => {
                        xpc_dictionary_set_uint64(dictionary, key.as_ptr(), *number)
                    }
                    WireValue::Dictionary(_) => {
                        let nested = xpc_object_of(entry);
                        xpc_dictionary_set_value(dictionary, key.as_ptr(), nested);
                        xpc_release(nested);
                    }
                }
            }
        }
    }
    dictionary
}
