//! `rgba_to_i420`: what an RGBA frame's colours become on the wire.

use tddy_screenshare::streamer::rgba_to_i420;

/// The luma and chroma of the first pixel of a 2×2 frame filled with `rgba`.
fn yuv_of(rgba: [u8; 4]) -> (u8, u8, u8) {
    let frame = rgba.repeat(4);
    let i420 = rgba_to_i420(&frame, 2, 2);
    let (y, u, v) = i420.data();
    (y[0], u[0], v[0])
}

#[test]
fn opaque_red_is_red_not_pink() {
    // When
    let (y, u, v) = yuv_of([255, 0, 0, 255]);

    // Then: BT.601 limited-range red is Y≈82, U≈90, V≈240
    assert!((80..=84).contains(&y), "luma {y}");
    assert!((88..=92).contains(&u), "blue chroma {u}");
    assert!((238..=242).contains(&v), "red chroma {v}");
}

#[test]
fn opaque_black_stays_black() {
    // When
    let (y, u, v) = yuv_of([0, 0, 0, 255]);

    // Then: alpha is not read as a colour
    assert_eq!((y, u, v), (16, 128, 128));
}
