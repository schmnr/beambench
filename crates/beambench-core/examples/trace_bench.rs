//! Standalone trace benchmark: PNG in, SVG out, node count on stdout.
//! Used to compare this engine against other tracers (e.g. Craftgineer
//! MonoTrace). Usage:
//!     cargo run -p beambench-core --example trace_bench -- input.png output.svg

use beambench_common::path::PathCommand;
use beambench_core::trace::{TraceConfig, trace_image};
use image::GrayImage;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: trace_bench <input.png> <output.svg>");
        std::process::exit(1);
    }

    let img = image::open(&args[1]).expect("failed to open input image");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Composite alpha onto white, then luma - same as the web service does.
    let mut gray = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = rgba.get_pixel(x, y);
            let a = p[3] as f64 / 255.0;
            let lum = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
            gray.put_pixel(x, y, image::Luma([(lum * a + 255.0 * (1.0 - a)) as u8]));
        }
    }

    let config = TraceConfig::default();
    let start = std::time::Instant::now();
    let paths = trace_image(&gray, &config);
    let elapsed = start.elapsed();

    let mut nodes = 0usize;
    let mut d_all = String::new();
    for path in &paths {
        for subpath in &path.subpaths {
            for cmd in &subpath.commands {
                if !matches!(cmd, PathCommand::Close) {
                    nodes += 1;
                }
            }
        }
        d_all.push_str(&path.to_svg_d());
        d_all.push(' ');
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\">\
         <path d=\"{}\" fill=\"black\" fill-rule=\"evenodd\"/></svg>",
        d_all.trim()
    );
    std::fs::write(&args[2], svg).expect("failed to write SVG");

    println!(
        "BeamBench: {} paths, {} nodes, {:.3}s",
        paths.len(),
        nodes,
        elapsed.as_secs_f64()
    );
}
