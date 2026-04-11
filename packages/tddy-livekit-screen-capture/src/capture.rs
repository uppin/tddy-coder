//! Enumerate monitors/windows and resolve capture targets (`monitor:<index>`, `window:<id>`).

use std::fmt::Write as _;

use anyhow::{Context, Result};
use image::RgbaImage;
use xcap::{Monitor, Window, XCapResult};

use crate::macos_access::{request_screen_capture_access, warn_if_screen_capture_denied};

/// Parsed CLI target (before resolving against current `Monitor::all()` / `Window::all()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetSpec {
    Monitor(usize),
    Window(u32),
}

/// Owning handle for repeated frame capture.
#[derive(Debug, Clone)]
pub enum StreamTarget {
    Monitor(Monitor),
    Window(Window),
}

impl StreamTarget {
    pub fn capture_image(&self) -> XCapResult<RgbaImage> {
        match self {
            StreamTarget::Monitor(m) => m.capture_image(),
            StreamTarget::Window(w) => w.capture_image(),
        }
    }

    pub fn width_height(&self) -> XCapResult<(u32, u32)> {
        match self {
            StreamTarget::Monitor(m) => Ok((m.width()?, m.height()?)),
            StreamTarget::Window(w) => Ok((w.width()?, w.height()?)),
        }
    }

    /// Human-readable fragment for LiveKit track name (sanitized).
    pub fn label_for_track(&self) -> Result<String> {
        let raw = match self {
            StreamTarget::Monitor(m) => m.friendly_name()?,
            StreamTarget::Window(w) => w.title()?,
        };
        Ok(sanitize_track_fragment(&raw))
    }
}

/// Parse `monitor:<usize>` or `window:<u32>`.
pub fn parse_target(s: &str) -> Result<TargetSpec> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("monitor:") {
        let n: usize = rest
            .parse()
            .with_context(|| format!("invalid monitor index in {:?}", s))?;
        return Ok(TargetSpec::Monitor(n));
    }
    if let Some(rest) = s.strip_prefix("window:") {
        let id: u32 = rest
            .parse()
            .with_context(|| format!("invalid window id in {:?}", s))?;
        return Ok(TargetSpec::Window(id));
    }
    anyhow::bail!(
        "invalid target {:?}: expected monitor:<index> or window:<id>",
        s
    );
}

pub fn resolve_target(spec: &TargetSpec) -> Result<StreamTarget> {
    match spec {
        TargetSpec::Monitor(index) => {
            let monitors = Monitor::all().context("failed to list monitors")?;
            let m = monitors.get(*index).with_context(|| {
                format!(
                    "monitor index {} out of range ({} monitors)",
                    index,
                    monitors.len()
                )
            })?;
            Ok(StreamTarget::Monitor(m.clone()))
        }
        TargetSpec::Window(id) => {
            for w in Window::all().context("failed to list windows")? {
                if w.id()? == *id {
                    return Ok(StreamTarget::Window(w));
                }
            }
            anyhow::bail!("no window with id {}", id);
        }
    }
}

fn sanitize_track_fragment(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().take(48) {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('_');
        }
    }
    if out.is_empty() {
        "capture".to_string()
    } else {
        out
    }
}

/// Multi-line listing for `--list`.
pub fn format_target_list() -> Result<String> {
    warn_wayland_linux();

    #[cfg(target_os = "macos")]
    {
        request_screen_capture_access();
    }

    let mut out = String::new();

    writeln!(out, "Monitors:").unwrap();
    let monitors = Monitor::all().context("failed to list monitors")?;
    for (i, m) in monitors.iter().enumerate() {
        let name = m.friendly_name().unwrap_or_else(|_| "?".to_string());
        let w = m.width().unwrap_or(0);
        let h = m.height().unwrap_or(0);
        writeln!(out, "  monitor:{}  {}  {}x{}", i, name, w, h).unwrap();
    }

    writeln!(out).unwrap();
    writeln!(out, "Windows:").unwrap();
    let windows = Window::all().context("failed to list windows")?;
    for w in windows {
        // Treat unknown state as not minimized: on macOS without Screen Recording,
        // `is_minimized()` often errors per-window and would hide almost everything.
        if w.is_minimized().unwrap_or(false) {
            continue;
        }
        let id = w.id()?;
        let title = w.title().unwrap_or_else(|_| String::new());
        let cw = w.width().unwrap_or(0);
        let ch = w.height().unwrap_or(0);
        writeln!(out, "  window:{}  {}  {}x{}", id, title, cw, ch).unwrap();
    }

    #[cfg(target_os = "macos")]
    warn_if_screen_capture_denied();

    Ok(out)
}

fn warn_wayland_linux() {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            log::warn!(
                "WAYLAND_DISPLAY is set: window capture may be limited; X11 session is recommended for full support."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_monitor_target() {
        assert_eq!(parse_target("monitor:0").unwrap(), TargetSpec::Monitor(0));
        assert_eq!(
            parse_target("  monitor:12 ").unwrap(),
            TargetSpec::Monitor(12)
        );
    }

    #[test]
    fn parse_window_target() {
        assert_eq!(parse_target("window:42").unwrap(), TargetSpec::Window(42));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_target("screen:1").is_err());
        assert!(parse_target("").is_err());
    }
}
