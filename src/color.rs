//! Color RGB de 8 bits por canal: predefinidos, parseo de nombre o hex, escala
//! de brillo y conversión HSV.

/// Color RGB con un byte por canal.
pub type Rgb = (u8, u8, u8);

/// Predefinidos aceptados por `parse_color`, en el orden que muestra la ayuda.
pub const PRESETS: &[(&str, Rgb)] = &[
    ("red", (255, 0, 0)),
    ("green", (0, 255, 0)),
    ("blue", (0, 0, 255)),
    ("cyan", (0, 255, 255)),
    ("magenta", (255, 0, 255)),
    ("yellow", (255, 255, 0)),
    ("orange", (255, 120, 0)),
    ("purple", (160, 0, 255)),
    ("pink", (255, 80, 160)),
    ("white", (255, 255, 255)),
    ("off", (0, 0, 0)),
];

/// Error de `parse_color`: conserva la entrada original para el mensaje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorError {
    input: String,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no se pudo interpretar el color: '{}' (usa hex 'ff6400' o un predefinido: {})",
            self.input,
            PRESETS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for ColorError {}

/// `true` si `b` es un dígito hex ASCII (`0-9` / `a-f`). Tras `to_lowercase`
/// basta con minúsculas; rechaza `+`, signos y cualquier no-ASCII.
fn is_ascii_hex_digit(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'a'..=b'f')
}

/// Interpreta `s` como predefinido (sin distinguir mayúsculas ni espacios
/// exteriores) o como hex de exactamente 6 dígitos ASCII con `#` opcional.
///
/// CLI-04: no se usa `from_str_radix` a ciegas — esa API admite `+` inicial
/// (`+fffff` tiene longitud 6 y parsearía). Se exige hex ASCII estricto.
/// Se conserva `#red` (y el resto de predefinidos con `#`).
pub fn parse_color(s: &str) -> Result<Rgb, ColorError> {
    let t = s.trim().to_lowercase();
    let t = t.strip_prefix('#').unwrap_or(t.as_str());
    if let Some(&(_, c)) = PRESETS.iter().find(|(n, _)| *n == t) {
        return Ok(c);
    }
    if t.len() == 6 && t.bytes().all(is_ascii_hex_digit) {
        // SAFETY lógica: seis hexdigits ASCII → `from_str_radix` no falla.
        let v = u32::from_str_radix(t, 16).map_err(|_| ColorError {
            input: s.to_string(),
        })?;
        return Ok((
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ));
    }
    Err(ColorError {
        input: s.to_string(),
    })
}

/// Escala cada canal por `pct` (0–100) con redondeo `(v*pct+50)/100`.
///
/// TEST-02: `pct` se satura a 100 antes de multiplicar, así `u32::MAX` y
/// valores >100 no desbordan en debug ni envuelven en release.
pub fn scale((r, g, b): Rgb, pct: u32) -> Rgb {
    let pct = pct.min(100);
    // Con pct≤100 el cociente cabe en u8 (máx. 255).
    let f = |v: u8| u8::try_from((u32::from(v) * pct + 50) / 100).unwrap_or(255);
    (f(r), f(g), f(b))
}

/// Convierte HSV (`h` en grados, `s` y `v` en 0–1) a `Rgb`.
#[allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let q = |t: f64| ((t + m) * 255.0 + 0.5) as u8;
    (q(r), q(g), q(b))
}

/// Formatea el color como hex minúsculo de 6 dígitos, sin `#`.
pub fn to_hex((r, g, b): Rgb) -> String {
    format!("{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::{hsv, parse_color, scale, to_hex};

    /// Generador xorshift64 determinista: sustituye a `proptest` sin dependencias.
    struct Xorshift64(u64);

    impl Xorshift64 {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn next_below(&mut self, bound: u32) -> u32 {
            u32::try_from(self.next() % u64::from(bound)).unwrap()
        }

        fn next_u8(&mut self) -> u8 {
            u8::try_from(self.next_below(256)).unwrap()
        }
    }

    #[test]
    fn parse_acepta_presets_hex_y_almohadilla() {
        assert_eq!(parse_color("red").unwrap(), (255, 0, 0));
        assert_eq!(parse_color(" Cyan ").unwrap(), (0, 255, 255));
        assert_eq!(parse_color("#red").unwrap(), (255, 0, 0));
        assert_eq!(parse_color("ff6400").unwrap(), (255, 100, 0));
        assert_eq!(parse_color("FF6400").unwrap(), (255, 100, 0));
        assert_eq!(parse_color("#FF6400").unwrap(), (255, 100, 0));
        assert!(parse_color("fucsia").is_err());
        assert!(parse_color("fff").is_err());
        assert!(parse_color("").is_err());
    }

    /// CLI-04: `from_str_radix` admitiría `+fffff`; el contrato exige 6 hex ASCII.
    #[test]
    fn parse_rechaza_signo_y_no_ascii_hex() {
        for bad in [
            "+fffff",
            "#+fffff",
            "+FF640",
            "-fffff",
            "gggggg",
            "ff640",
            "ff64000",
            "ff 640",
            "ＦＦＦＦＦＦ",
            "١٢٣٤٥٦",
        ] {
            assert!(parse_color(bad).is_err(), "debió rechazarse {bad:?}");
        }
    }

    #[test]
    fn scale_redondea_y_satura_pct() {
        assert_eq!(scale((255, 255, 255), 100), (255, 255, 255));
        assert_eq!(scale((255, 120, 0), 50), (128, 60, 0));
        assert_eq!(scale((255, 255, 255), 0), (0, 0, 0));
        assert_eq!(scale((1, 1, 1), 50), (1, 1, 1));
        // TEST-02: pct fuera de 0–100 se satura a 100 (no overflow).
        assert_eq!(scale((255, 120, 0), 101), (255, 120, 0));
        assert_eq!(scale((255, 120, 0), u32::MAX), (255, 120, 0));
        assert_eq!(scale((10, 20, 30), 1_000_000), (10, 20, 30));
    }

    #[test]
    fn hsv_primarios_gris_negro_y_sectores() {
        assert_eq!(hsv(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv(240.0, 1.0, 1.0), (0, 0, 255));
        assert_eq!(hsv(0.0, 0.0, 1.0), (255, 255, 255));
        // TEST-01: vectores independientes (no tautología u8→u16).
        assert_eq!(hsv(0.0, 0.0, 0.0), (0, 0, 0));
        assert_eq!(hsv(180.0, 0.0, 0.5), (128, 128, 128));
        // Canal fijo a 0 en cada sector (límites inclusive).
        assert_eq!(hsv(0.0, 1.0, 1.0).2, 0);
        assert_eq!(hsv(59.0, 1.0, 1.0).2, 0);
        assert_eq!(hsv(60.0, 1.0, 1.0).2, 0);
        assert_eq!(hsv(120.0, 1.0, 1.0).0, 0);
        assert_eq!(hsv(179.0, 1.0, 1.0).0, 0);
        assert_eq!(hsv(180.0, 1.0, 1.0).0, 0);
        assert_eq!(hsv(240.0, 1.0, 1.0).1, 0);
        assert_eq!(hsv(300.0, 1.0, 1.0).1, 0);
        let mut broken_like = hsv(0.0, 1.0, 1.0);
        broken_like.0 = broken_like.0.wrapping_add(1);
        assert_ne!(broken_like, (255, 0, 0));
    }

    #[test]
    fn parse_color_a_to_hex_ida_y_vuelta() {
        let mut rng = Xorshift64(0x1234_5678_9abc_def0);
        for _ in 0..10_000 {
            let v = rng.next_below(1 << 24);
            let hex = format!("{v:06x}");
            assert_eq!(to_hex(parse_color(&hex).unwrap()), hex);
        }
    }

    #[test]
    fn scale_invariantes_aleatorias() {
        let mut rng = Xorshift64(0x0f1e_2d3c_4b5a_6978);
        for _ in 0..10_000 {
            let c = (rng.next_u8(), rng.next_u8(), rng.next_u8());
            let a = rng.next_below(101);
            let b = rng.next_below(101);
            let (p1, p2) = (a.min(b), a.max(b));
            for pct in [0, 100, p1, p2] {
                let s = scale(c, pct);
                // Oráculo de la fórmula (sustituye la tautología u8≤255).
                let expect = |v: u8| u8::try_from((u32::from(v) * pct + 50) / 100).unwrap_or(255);
                assert_eq!(s, (expect(c.0), expect(c.1), expect(c.2)));
            }
            assert_eq!(scale(c, 0), (0, 0, 0));
            assert_eq!(scale(c, 100), c);
            assert_eq!(scale(c, 250), c); // saturación a 100
            let (s1, s2) = (scale(c, p1), scale(c, p2));
            assert!(s1.0 <= s2.0 && s1.1 <= s2.1 && s1.2 <= s2.2);
        }
    }

    #[test]
    fn hsv_invariantes_aleatorias() {
        let mut rng = Xorshift64(0xfeed_face_cafe_beef);
        for _ in 0..10_000 {
            let h = f64::from(rng.next_below(360_000)) / 1000.0;
            let s = f64::from(rng.next_below(1001)) / 1000.0;
            let v = f64::from(rng.next_below(1001)) / 1000.0;
            assert!(h < 360.0 && s <= 1.0 && v <= 1.0);
            let c = hsv(h, s, v);
            if v == 0.0 {
                assert_eq!(c, (0, 0, 0));
            }
            if s == 0.0 {
                assert_eq!(c.0, c.1);
                assert_eq!(c.1, c.2);
            }
            if v >= 1.0 && s > 0.0 {
                assert!(c.0 == 255 || c.1 == 255 || c.2 == 255);
            }
            let full = hsv(h, s, 1.0);
            if s > 0.0 {
                assert!(full.0 == 255 || full.1 == 255 || full.2 == 255);
            } else {
                assert_eq!(full, (255, 255, 255));
            }
        }
    }
}
