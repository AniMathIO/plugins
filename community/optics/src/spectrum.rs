//! Wavelength-dependent refraction and spectral colours for the dispersion
//! setup. The index model is a two-term Cauchy equation calibrated so the
//! 550 nm index equals the plugin's `index` parameter. The colour mapping is
//! the standard piecewise approximation of the spectral locus (400–700 nm);
//! it is a display approximation, not a colorimetric conversion.

/// Cauchy dispersion strength (µm²) used for the glass; a middle value between
/// crown and flint glasses, chosen for a clearly visible prism spread.
const CAUCHY_B: f64 = 0.010;
/// Reference wavelength (µm) the plugin's `index` parameter is calibrated at.
const REFERENCE_UM: f64 = 0.55;

/// Refractive index at `lambda_nm` for a glass whose 550 nm index is `base`.
pub fn index_at(base: f64, lambda_nm: f64) -> f64 {
    let lambda_um = lambda_nm / 1000.0;
    let a = base - CAUCHY_B / (REFERENCE_UM * REFERENCE_UM);
    (a + CAUCHY_B / (lambda_um * lambda_um)).max(1.0)
}

/// Approximate sRGB colour of a monochromatic 400–700 nm light, gamma-naive
/// and clamped. Outside the range the nearest end colour is returned.
pub fn wavelength_rgb(lambda_nm: f64) -> (u8, u8, u8) {
    let lambda_nm = lambda_nm.clamp(400.0, 700.0);
    let (r, g, b) = raw_rgb(lambda_nm);
    let intensity = if lambda_nm > 660.0 {
        1.0 - 0.35 * (lambda_nm - 660.0) / 40.0
    } else if lambda_nm < 430.0 {
        1.0 - 0.35 * (430.0 - lambda_nm) / 30.0
    } else {
        1.0
    };
    let channel = |v: f64| (255.0 * intensity * v).round() as u8;
    (channel(r), channel(g), channel(b))
}

fn raw_rgb(lambda_nm: f64) -> (f64, f64, f64) {
    let (r, g, b) = if (380.0..440.0).contains(&lambda_nm) {
        (-(lambda_nm - 440.0) / 60.0, 0.0, 1.0)
    } else if (440.0..490.0).contains(&lambda_nm) {
        (0.0, (lambda_nm - 440.0) / 50.0, 1.0)
    } else if (490.0..510.0).contains(&lambda_nm) {
        (0.0, 1.0, -(lambda_nm - 510.0) / 20.0)
    } else if (510.0..580.0).contains(&lambda_nm) {
        ((lambda_nm - 510.0) / 70.0, 1.0, 0.0)
    } else if (580.0..645.0).contains(&lambda_nm) {
        (1.0, -(lambda_nm - 645.0) / 65.0, 0.0)
    } else if (645.0..780.0).contains(&lambda_nm) {
        (1.0, 0.0, 0.0)
    } else if lambda_nm < 380.0 {
        (0.0, 0.0, 1.0)
    } else {
        (1.0, 0.0, 0.0)
    };
    (r, g, b)
}

/// The dispersion sample wavelengths for `count` rays, violet 400 nm first and
/// red 700 nm last, evenly spaced. `count` is clamped to 1..=64.
pub fn sample_wavelengths(count: usize) -> Vec<f64> {
    let count = count.clamp(1, MAX_SPECTRAL_RAYS);
    if count == 1 {
        return vec![REFERENCE_UM * 1000.0];
    }
    (0..count)
        .map(|i| 400.0 + (700.0 - 400.0) * i as f64 / (count - 1) as f64)
        .collect()
}

pub const MAX_SPECTRAL_RAYS: usize = 64;
