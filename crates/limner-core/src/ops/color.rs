//! Colour arithmetic for the Colorize tool.
//!
//! Colours are stored as linear light. The blend modes are the ones painters know
//! from image editors, where they work on display-encoded values, so each blend
//! encodes its inputs, combines them, and decodes the result. That keeps "multiply by
//! mid grey" meaning what an artist expects it to mean.

use serde::{Deserialize, Serialize};

/// How the Colorize tool combines the paint colour with a stroke's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Darken,
    Multiply,
    Overlay,
    Burn,
    Lighten,
    Screen,
    Add,
    Dodge,
    Hue,
    Saturation,
    Color,
    Luminosity,
    /// Changes how see-through the stroke is instead of its colour.
    Opacity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 14] = [
        BlendMode::Normal,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::Overlay,
        BlendMode::Burn,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::Add,
        BlendMode::Dodge,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::Opacity,
    ];

    pub fn title(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal (RGB)",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::Overlay => "Overlay",
            BlendMode::Burn => "Burn",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::Add => "Add",
            BlendMode::Dodge => "Dodge",
            BlendMode::Hue => "Hue (H)",
            BlendMode::Saturation => "Saturation (S)",
            BlendMode::Color => "Color (HS)",
            BlendMode::Luminosity => "Luminosity (V)",
            BlendMode::Opacity => "Opacity (A)",
        }
    }
}

/// Linear light to the sRGB display encoding.
pub fn encode(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The sRGB display encoding to linear light.
pub fn decode(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Hue (0 to 1), saturation and value of an RGB colour.
pub fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    [hue, if max <= 0.0 { 0.0 } else { delta / max }, max]
}

pub fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = hsv;
    let h = h.rem_euclid(1.0) * 6.0;
    let sector = h.floor();
    let f = h - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match sector as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// The colour `mode` makes of `base` (a stroke's colour) and `paint`, at full
/// strength. Both are linear RGB, and so is the result.
pub fn blend(mode: BlendMode, base: [f32; 3], paint: [f32; 3]) -> [f32; 3] {
    if matches!(mode, BlendMode::Normal) {
        return paint;
    }
    if matches!(mode, BlendMode::Opacity) {
        return base;
    }
    let b = base.map(|c| encode(c).min(1.0));
    let p = paint.map(|c| encode(c).min(1.0));
    let each = |f: fn(f32, f32) -> f32| [f(b[0], p[0]), f(b[1], p[1]), f(b[2], p[2])];
    let (hb, hp) = (rgb_to_hsv(b), rgb_to_hsv(p));
    let out = match mode {
        BlendMode::Normal | BlendMode::Opacity => unreachable!("handled above"),
        BlendMode::Darken => each(f32::min),
        BlendMode::Multiply => each(|b, p| b * p),
        BlendMode::Overlay => each(|b, p| {
            if b < 0.5 {
                2.0 * b * p
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - p)
            }
        }),
        BlendMode::Burn => each(|b, p| {
            if p <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / p).min(1.0)
            }
        }),
        BlendMode::Lighten => each(f32::max),
        BlendMode::Screen => each(|b, p| 1.0 - (1.0 - b) * (1.0 - p)),
        BlendMode::Add => each(|b, p| (b + p).min(1.0)),
        BlendMode::Dodge => each(|b, p| {
            if p >= 1.0 {
                1.0
            } else {
                (b / (1.0 - p)).min(1.0)
            }
        }),
        BlendMode::Hue => hsv_to_rgb([hp[0], hb[1], hb[2]]),
        // A grey has no hue of its own to keep, so saturating it takes the paint's hue.
        BlendMode::Saturation => {
            hsv_to_rgb([if hb[1] <= 0.0 { hp[0] } else { hb[0] }, hp[1], hb[2]])
        }
        BlendMode::Color => hsv_to_rgb([hp[0], hp[1], hb[2]]),
        BlendMode::Luminosity => hsv_to_rgb([hb[0], hb[1], hp[2]]),
    };
    out.map(decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn encoding_round_trips() {
        for c in [0.0, 0.001, 0.02, 0.2, 0.5, 1.0] {
            assert!((decode(encode(c)) - c).abs() < 1e-6);
        }
        assert!(
            (encode(0.214) - 0.5).abs() < 2e-3,
            "mid grey on screen is about 0.21 in linear light"
        );
    }

    #[test]
    fn hsv_round_trips_and_names_the_primaries() {
        assert_eq!(rgb_to_hsv([1.0, 0.0, 0.0]), [0.0, 1.0, 1.0]);
        assert!(close(rgb_to_hsv([0.0, 0.5, 0.0]), [1.0 / 3.0, 1.0, 0.5]));
        assert!(close(rgb_to_hsv([0.0, 0.0, 1.0]), [2.0 / 3.0, 1.0, 1.0]));
        assert_eq!(rgb_to_hsv([0.3, 0.3, 0.3]), [0.0, 0.0, 0.3]);
        for rgb in [
            [0.9, 0.2, 0.4],
            [0.1, 0.8, 0.7],
            [0.5, 0.5, 0.1],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ] {
            assert!(close(hsv_to_rgb(rgb_to_hsv(rgb)), rgb), "{rgb:?}");
        }
    }

    #[test]
    fn arithmetic_modes_behave_as_in_an_image_editor() {
        let (white, black, grey) = ([1.0; 3], [0.0; 3], [decode(0.5); 3]);
        let red = [1.0, 0.0, 0.0];
        assert!(close(blend(BlendMode::Normal, grey, red), red));
        assert!(
            close(blend(BlendMode::Multiply, red, white), red),
            "white is neutral for multiply"
        );
        assert!(close(blend(BlendMode::Multiply, white, grey), grey));
        assert!(
            close(blend(BlendMode::Multiply, grey, grey), [decode(0.25); 3]),
            "half of half on screen"
        );
        assert!(
            close(blend(BlendMode::Screen, red, black), red),
            "black is neutral for screen"
        );
        assert!(close(
            blend(BlendMode::Screen, grey, grey),
            [decode(0.75); 3]
        ));
        assert!(close(blend(BlendMode::Darken, grey, white), grey));
        assert!(close(blend(BlendMode::Lighten, grey, white), white));
        assert!(close(blend(BlendMode::Add, grey, grey), white));
        assert!(
            close(blend(BlendMode::Overlay, grey, grey), grey),
            "mid grey is neutral for overlay"
        );
        assert!(close(blend(BlendMode::Dodge, grey, grey), white));
        assert!(close(blend(BlendMode::Burn, grey, grey), black));
        assert!(
            close(blend(BlendMode::Dodge, grey, black), grey),
            "black is neutral for dodge"
        );
        assert!(
            close(blend(BlendMode::Burn, grey, white), grey),
            "white is neutral for burn"
        );
    }

    #[test]
    fn hsv_modes_swap_single_components() {
        let teal = [0.0, decode(0.5), decode(0.5)];
        let red = [1.0, 0.0, 0.0];
        assert!(
            close(blend(BlendMode::Hue, teal, red), [decode(0.5), 0.0, 0.0]),
            "red, as dark as the teal was"
        );
        assert!(close(
            blend(BlendMode::Color, teal, red),
            [decode(0.5), 0.0, 0.0]
        ));
        assert!(
            close(blend(BlendMode::Luminosity, teal, red), [0.0, 1.0, 1.0]),
            "teal, as bright as the red"
        );
        let grey = [decode(0.4); 3];
        assert!(
            close(
                blend(BlendMode::Saturation, grey, red),
                [decode(0.4), 0.0, 0.0]
            ),
            "a grey takes the paint's hue when saturated"
        );
        assert!(
            close(blend(BlendMode::Saturation, teal, grey), [decode(0.5); 3]),
            "saturating with a grey drains the colour"
        );
        assert!(
            close(blend(BlendMode::Opacity, teal, red), teal),
            "opacity mode leaves colour alone"
        );
    }
}
