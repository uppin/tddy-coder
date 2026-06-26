//! Shared pixel and keysym helpers.
//!
//! Ported from ~/Code/makers-lt/common/vnc-livekit/src/common.rs.

/// Convert an RGBA pixel buffer to ABGR in-place (required by LiveKit's yuv_helper).
pub fn rgba_to_abgr(pixels: &[u8]) -> Vec<u8> {
    let mut abgr = Vec::with_capacity(pixels.len());
    for chunk in pixels.chunks_exact(4) {
        abgr.push(chunk[3]); // A
        abgr.push(chunk[2]); // B
        abgr.push(chunk[1]); // G
        abgr.push(chunk[0]); // R
    }
    abgr
}

/// Map an ASCII character to its X11 keysym.
///
/// Returns `None` for characters outside the printable ASCII range or without a
/// direct keysym mapping.
pub fn char_to_keysym(c: char) -> Option<u32> {
    match c {
        ' '..='~' => Some(c as u32),
        '\r' | '\n' => Some(0xff0d), // XK_Return
        '\t' => Some(0xff09),        // XK_Tab
        '\x08' => Some(0xff08),      // XK_BackSpace
        '\x1b' => Some(0xff1b),      // XK_Escape
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_to_abgr_swaps_channels() {
        // Given RGBA pixel [R, G, B, A]
        let rgba = vec![10u8, 20, 30, 40];
        // When
        let abgr = rgba_to_abgr(&rgba);
        // Then [A, B, G, R]
        assert_eq!(abgr, vec![40, 30, 20, 10]);
    }

    #[test]
    fn char_to_keysym_ascii_passthrough() {
        assert_eq!(char_to_keysym('A'), Some(0x41));
        assert_eq!(char_to_keysym(' '), Some(0x20));
    }

    #[test]
    fn char_to_keysym_special_keys() {
        assert_eq!(char_to_keysym('\r'), Some(0xff0d));
        assert_eq!(char_to_keysym('\t'), Some(0xff09));
    }
}
