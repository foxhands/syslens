//! The program's pictures, drawn by the program itself: the window icon and
//! the small load meter shown next to the clock.

/// Whether pixel (x, y) lies inside a rounded square from `a` to `b` (exclusive).
fn inside(x: i32, y: i32, a: i32, b: i32, radius: i32) -> bool {
    if x < a || y < a || x >= b || y >= b {
        return false;
    }
    let near = |v: i32| if v < a + radius { a + radius - v } else if v >= b - radius { v - (b - radius - 1) } else { 0 };
    let (dx, dy) = (near(x), near(y));
    dx * dx + dy * dy <= radius * radius
}

fn put(px: &mut [u8], size: u32, x: i32, y: i32, c: [u8; 4]) {
    let i = (y as usize * size as usize + x as usize) * 4;
    px[i..i + 4].copy_from_slice(&c);
}

/// The SysLens mark: a blue rounded square with a white one inside. RGBA.
pub fn app(size: u32) -> Vec<u8> {
    let n = size as i32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            if inside(x, y, n * 5 / 16, n * 11 / 16, n / 16) {
                put(&mut px, size, x, y, [255, 255, 255, 235]);
            } else if inside(x, y, 0, n, n * 9 / 32) {
                put(&mut px, size, x, y, [84, 118, 245, 255]);
            }
        }
    }
    px
}

/// Side of the load meter, pixels.
#[cfg_attr(not(windows), allow(dead_code))]
pub const METER: u32 = 32;

/// The load meter: two bars on a dark square, processor (blue) on the left
/// and memory (green) on the right; shares are 0..1. A nearly full bar is red.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn meter(cpu: f32, memory: f32) -> Vec<u8> {
    let n = METER as i32;
    let mut px = vec![0u8; (METER * METER * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            if inside(x, y, 0, n, 6) {
                put(&mut px, METER, x, y, [24, 26, 32, 255]);
            }
        }
    }
    let bars = [(cpu, 5, [108, 140, 255, 255]), (memory, 18, [80, 200, 120, 255])];
    for (share, left, colour) in bars {
        let share = if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 };
        let colour = if share >= 0.9 { [235, 90, 90, 255] } else { colour };
        let (top, bottom) = (4, n - 4);
        let filled = ((bottom - top) as f32 * share).round().max(2.0) as i32;
        for y in top..bottom {
            for x in left..left + 9 {
                put(&mut px, METER, x, y, if y >= bottom - filled { colour } else { [58, 62, 74, 255] });
            }
        }
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(px: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn pictures() {
        let icon = app(64);
        assert_eq!(icon.len(), 64 * 64 * 4);
        assert_eq!(at(&icon, 64, 0, 0)[3], 0); // rounded corner
        assert_eq!(at(&icon, 64, 32, 32), [255, 255, 255, 235]);
        assert_eq!(at(&icon, 64, 32, 6), [84, 118, 245, 255]);

        let m = meter(0.5, 0.95);
        assert_eq!(m.len(), (METER * METER * 4) as usize);
        assert_eq!(at(&m, METER, 8, 26), [108, 140, 255, 255]); // processor: lower half filled
        assert_eq!(at(&m, METER, 8, 6), [58, 62, 74, 255]);
        assert_eq!(at(&m, METER, 22, 6), [235, 90, 90, 255]); // memory nearly full: red to the top
        assert_eq!(meter(f32::NAN, 2.0).len(), m.len());
    }
}
