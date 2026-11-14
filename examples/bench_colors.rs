//! Benchmark mínimo, solo std, de las operaciones puras del CLI.
//!
//! Ejecutar: `cargo run --release --example bench_colors`.
//!
//! Cada línea mide el tiempo medio por operación con `Instant`; las entradas
//! varían en cada iteración y pasan por `std::hint::black_box` para que el
//! optimizador no pliegue ni eleve las llamadas fuera del bucle. Los módulos
//! del binario se incluyen con `#[path]` porque el paquete no expone una
//! biblioteca.

#![allow(dead_code)]

#[path = "../src/color.rs"]
mod color;
#[path = "../src/protocol.rs"]
mod protocol;

use std::hint::black_box;
use std::time::{Duration, Instant};

use color::{hsv, parse_color, scale, to_hex};
use protocol::{autonomous_report, color_report, hid_ioctl_request, HIDIOCGFEATURE_NR};

const ITERS: u32 = 500_000;
const COLORS: [&str; 4] = ["ff6400", "#00ff00", "cyan", "A1b2C3"];
const RGBS: [(u8, u8, u8); 4] = [(255, 100, 0), (0, 255, 128), (17, 34, 51), (255, 255, 255)];

fn print_row(name: &str, iters: u32, elapsed: Duration) {
    let ns = elapsed.as_secs_f64() * 1_000_000_000.0 / f64::from(iters);
    println!("{name:<20} {ns:>10.1} ns/op  ({iters} iteraciones)");
}

fn bench<T>(name: &str, iters: u32, mut f: impl FnMut() -> T) {
    let start = Instant::now();
    for _ in 0..iters {
        black_box(f());
    }
    print_row(name, iters, start.elapsed());
}

fn main() {
    println!("iLutex — operaciones puras (std, release)");

    let mut t = 0u32;
    bench("parse_color", ITERS, || {
        t = t.wrapping_add(1);
        let s = COLORS[usize::try_from(t % 4).unwrap()];
        parse_color(black_box(s)).unwrap()
    });

    let mut t = 0u32;
    bench("scale", ITERS, || {
        t = t.wrapping_add(1);
        let c = RGBS[usize::try_from(t % 4).unwrap()];
        scale(black_box(c), black_box(t % 101))
    });

    let mut t = 0u32;
    bench("hsv", ITERS, || {
        t = t.wrapping_add(1);
        hsv(
            black_box(f64::from(t % 360)),
            black_box(0.75),
            black_box(0.9),
        )
    });

    let mut t = 0u32;
    bench("to_hex", ITERS, || {
        t = t.wrapping_add(1);
        to_hex(black_box(RGBS[usize::try_from(t % 4).unwrap()]))
    });

    let mut t = 0u32;
    bench("color_report", ITERS, || {
        t = t.wrapping_add(1);
        color_report(
            black_box(u16::try_from(t % 1024).unwrap()),
            black_box(RGBS[usize::try_from(t % 4).unwrap()]),
        )
    });

    let mut t = 0u32;
    bench("autonomous_report", ITERS, || {
        t = t.wrapping_add(1);
        autonomous_report(black_box(t.is_multiple_of(2)))
    });

    let mut t = 0u32;
    bench("hid_ioctl_request", ITERS, || {
        t = t.wrapping_add(1);
        hid_ioctl_request(
            black_box(HIDIOCGFEATURE_NR),
            black_box(usize::try_from(t % 64 + 1).unwrap()),
        )
        .unwrap()
    });
}
