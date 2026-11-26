//! The production window port on macOS: xcap finds and captures the window showing the device, and
//! its accessibility tree says where the device is in it. Input goes to the device itself
//! (`crate::dtuhid_xpc`).

use anyhow::Context as _;
use objc2_core_graphics::CGPreflightScreenCaptureAccess;

use crate::geometry::{below_title_bar, Rect};
use crate::window::{Frame, SimulatorWindow, SimulatorWindows};

/// The window owners that show simulator devices, as macOS names them (`kCGWindowOwnerName`, the
/// process name): `Simulator` up to Xcode 26, and `Device Hub` from Xcode 27, where DeviceHub.app
/// replaced Simulator.app. Both title a device window `<name> – <runtime>`.
const SIMULATOR_APPS: [&str; 2] = ["Simulator", "Device Hub"];

/// Simulator.app's owner name: its window is the device below a title bar.
const SIMULATOR_APP: &str = "Simulator";

/// Height of a Simulator.app window's title bar, in points.
const TITLE_BAR_POINTS: f64 = 28.0;

/// Fail unless this process may capture the screen and read the window's accessibility tree:
/// without Screen Recording the capture is blank, without Accessibility Device Hub's device area
/// cannot be found.
pub(crate) fn require_permissions() -> anyhow::Result<()> {
    if !CGPreflightScreenCaptureAccess() {
        anyhow::bail!(
            "Screen Recording permission is not granted to this process: allow it in System \
             Settings › Privacy & Security › Screen Recording"
        );
    }
    // SAFETY: `AXIsProcessTrusted` takes no arguments and only reads this process's trust state.
    if !unsafe { objc2_application_services::AXIsProcessTrusted() } {
        anyhow::bail!(
            "Accessibility permission is not granted to this process: allow it in System \
             Settings › Privacy & Security › Accessibility"
        );
    }
    Ok(())
}

/// Simulator.app windows, through xcap.
#[derive(Debug, Default, Clone, Copy)]
pub struct XcapSimulatorWindows;

impl XcapSimulatorWindows {
    fn window_by_id(id: u32) -> anyhow::Result<xcap::Window> {
        xcap::Window::all()
            .context("listing windows")?
            .into_iter()
            .find(|window| window.id().ok() == Some(id))
            .with_context(|| format!("the Simulator.app window {id} is gone"))
    }
}

impl SimulatorWindows for XcapSimulatorWindows {
    fn find(&self, device_name: &str) -> anyhow::Result<Option<SimulatorWindow>> {
        for window in xcap::Window::all().context("listing windows")? {
            let shows_devices = window
                .app_name()
                .is_ok_and(|app| SIMULATOR_APPS.contains(&app.as_str()));
            if !shows_devices {
                continue;
            }
            let title = window.title().context("reading a window title")?;
            if !titles_device(&title, device_name) {
                continue;
            }
            let scale = window
                .current_monitor()
                .and_then(|monitor| monitor.scale_factor())
                .context("reading the Simulator window's display scale")?;
            let bounds = window_bounds(&window)?;
            let owner = window.app_name().context("reading the window owner")?;
            // Each app lays its window out its own way: Simulator.app shows the device below a
            // title bar; Device Hub shows it beside a sidebar and below a toolbar, at a place only
            // its accessibility tree says.
            let device_area = if owner == SIMULATOR_APP {
                below_title_bar(bounds, TITLE_BAR_POINTS)
            } else {
                let pid = window.pid().context("reading the window's process")?;
                accessibility::device_area(pid, &title, bounds)?.with_context(|| {
                    format!(
                        "{owner} shows no iOS screen for {device_name} in its window \"{title}\""
                    )
                })?
            };
            // TODO(ios-simulator): the bounds and device area are read once; a window moved or
            // resized after connect receives input at its old position.
            return Ok(Some(SimulatorWindow {
                id: window.id().context("reading a window id")?,
                owner_pid: window.pid().context("reading the window's process")?,
                title,
                bounds,
                device_area,
                scale: f64::from(scale),
            }));
        }
        Ok(None)
    }

    fn capture(&self, window: &SimulatorWindow) -> anyhow::Result<Frame> {
        let image = Self::window_by_id(window.id)?
            .capture_image()
            .context("capturing the Simulator.app window")?;
        Ok(Frame {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }
}

/// Where `window` is on screen, in points.
fn window_bounds(window: &xcap::Window) -> anyhow::Result<Rect> {
    Ok(Rect {
        x: f64::from(window.x().context("reading the window position")?),
        y: f64::from(window.y().context("reading the window position")?),
        width: f64::from(window.width().context("reading the window size")?),
        height: f64::from(window.height().context("reading the window size")?),
    })
}

/// Whether a window titled `title` shows the device `device_name`: the title is the name alone or
/// `<name> – <runtime>`, so "iPhone 16" does not match an "iPhone 16 Pro" window.
fn titles_device(title: &str, device_name: &str) -> bool {
    title
        .strip_prefix(device_name)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(" –") || rest.starts_with(" -"))
}

/// Where a window shows its device, read from its owner's accessibility tree.
mod accessibility {
    use std::collections::VecDeque;
    use std::ffi::c_void;
    use std::ptr::{self, NonNull};

    use anyhow::Context as _;
    use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
    use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType, CGPoint, CGSize};

    use crate::geometry::Rect;

    /// The subrole Device Hub gives the group that is the iOS screen itself.
    const IOS_CONTENT_SUBROLE: &str = "iOSContentGroup";

    /// How deep below the window the iOS screen is looked for, and how many elements at most are
    /// visited: the tree under a device is deep (every icon on the home screen is in it), and the
    /// group sought sits near its top.
    const MAX_DEPTH: usize = 12;
    const MAX_VISITED: usize = 4_000;

    /// How long one accessibility call may take before it is given up on, in seconds.
    const MESSAGING_TIMEOUT_SECONDS: f32 = 1.0;

    /// How far apart, in points, an accessibility window frame and a window-server frame may be and
    /// still be the same window.
    const SAME_FRAME_TOLERANCE: f64 = 1.0;

    /// The device area of the window titled `title` at `bounds`, owned by process `pid`; `None`
    /// when that window shows no iOS screen.
    pub(super) fn device_area(pid: u32, title: &str, bounds: Rect) -> anyhow::Result<Option<Rect>> {
        let pid = i32::try_from(pid).context("a window owner's process id")?;
        // SAFETY: creating an accessibility handle for a process id has no preconditions.
        let app = unsafe { AXUIElement::new_application(pid) };
        // SAFETY: `app` is a valid accessibility element.
        unsafe { app.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS) };
        let window = elements(&app, "AXWindows")
            .into_iter()
            .find(|window| {
                string(window, "AXTitle").as_deref() == Some(title)
                    || frame(window).is_some_and(|found| same_frame(found, bounds))
            })
            .with_context(|| format!("no accessible window titled \"{title}\""))?;
        Ok(ios_content(&window).and_then(|content| frame(&content)))
    }

    /// The first element at most [`MAX_DEPTH`] below `root` whose subrole is the iOS screen's.
    fn ios_content(root: &AXUIElement) -> Option<CFRetained<AXUIElement>> {
        let mut queue: VecDeque<(CFRetained<AXUIElement>, usize)> = elements(root, "AXChildren")
            .into_iter()
            .map(|child| (child, 1))
            .collect();
        let mut visited = 0;
        while let Some((element, depth)) = queue.pop_front() {
            visited += 1;
            if visited > MAX_VISITED {
                return None;
            }
            if string(&element, "AXSubrole").as_deref() == Some(IOS_CONTENT_SUBROLE) {
                return Some(element);
            }
            if depth < MAX_DEPTH {
                queue.extend(
                    elements(&element, "AXChildren")
                        .into_iter()
                        .map(|child| (child, depth + 1)),
                );
            }
        }
        None
    }

    fn same_frame(a: Rect, b: Rect) -> bool {
        [
            (a.x, b.x),
            (a.y, b.y),
            (a.width, b.width),
            (a.height, b.height),
        ]
        .iter()
        .all(|(x, y)| (x - y).abs() <= SAME_FRAME_TOLERANCE)
    }

    /// `element`'s attribute `name`, or `None` when it has none or it could not be read.
    fn attribute(element: &AXUIElement, name: &'static str) -> Option<CFRetained<CFType>> {
        let name = CFString::from_static_str(name);
        let mut value: *const CFType = ptr::null();
        // SAFETY: `value` is a valid place for the copied value, which is returned retained.
        let error = unsafe { element.copy_attribute_value(&name, NonNull::from(&mut value)) };
        if error != AXError::Success {
            return None;
        }
        // SAFETY: a successful copy hands over a +1 reference, which `from_raw` takes.
        NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
    }

    fn string(element: &AXUIElement, name: &'static str) -> Option<String> {
        let value = attribute(element, name)?.downcast::<CFString>().ok()?;
        Some(value.to_string())
    }

    /// The elements an array attribute lists (`AXWindows`, `AXChildren`).
    fn elements(element: &AXUIElement, name: &'static str) -> Vec<CFRetained<AXUIElement>> {
        let Some(array) =
            attribute(element, name).and_then(|value| value.downcast::<CFArray>().ok())
        else {
            return Vec::new();
        };
        (0..array.len())
            .filter_map(|index| {
                // SAFETY: `index` is below the array's length.
                let item = unsafe { array.value_at_index(index as _) };
                // SAFETY: these attributes list accessibility elements; retaining one keeps it
                // alive past the array.
                NonNull::new(item.cast_mut().cast::<AXUIElement>())
                    .map(|item| unsafe { CFRetained::retain(item) })
            })
            .collect()
    }

    /// `element`'s on-screen frame in points, from its `AXPosition` and `AXSize`.
    fn frame(element: &AXUIElement) -> Option<Rect> {
        let mut origin = CGPoint { x: 0.0, y: 0.0 };
        let mut size = CGSize {
            width: 0.0,
            height: 0.0,
        };
        let read = value(element, "AXPosition", AXValueType::CGPoint, &mut origin)
            && value(element, "AXSize", AXValueType::CGSize, &mut size);
        read.then_some(Rect {
            x: origin.x,
            y: origin.y,
            width: size.width,
            height: size.height,
        })
    }

    /// Read `element`'s `AXValue` attribute `name`, of type `kind`, into `out`.
    fn value<T>(element: &AXUIElement, name: &'static str, kind: AXValueType, out: &mut T) -> bool {
        let Some(value) =
            attribute(element, name).and_then(|value| value.downcast::<AXValue>().ok())
        else {
            return false;
        };
        // SAFETY: `out` is a `T` matching `kind` (`CGPoint` / `CGSize`), written in place.
        unsafe { value.value(kind, NonNull::from(out).cast::<c_void>()) }
    }
}

#[cfg(test)]
mod tests {
    use super::titles_device;

    #[test]
    fn a_window_titled_for_a_longer_device_name_is_not_that_device() {
        assert!(titles_device("iPhone 16 – iOS 18.2", "iPhone 16"));
        assert!(titles_device("iPhone 16", "iPhone 16"));
        assert!(
            !titles_device("iPhone 16 Pro – iOS 18.2", "iPhone 16"),
            "\"iPhone 16\" must not claim the iPhone 16 Pro's window"
        );
    }
}
