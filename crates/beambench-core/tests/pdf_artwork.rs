//! Generated PDFs compared with independently rendered Poppler references.
//! Regenerate explicitly with PDFTOPPM=/path/to/pdftoppm PDF_REGENERATE=1
//! cargo test -p beambench-core --test pdf_artwork -- --nocapture.
use base64::Engine;
use beambench_core::{PdfArtwork, PdfPaintMode, PdfRgbColor, parse_pdf_artwork};
use lopdf::{Document, Object, Stream, dictionary};
use std::path::PathBuf;

const MM: f64 = 25.4 / 72.0;

fn fixture() -> Document {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 4, "Height" => 3,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        },
        vec![0, 0, 0, 255, 0, 0, 255, 255, 0, 255, 255, 255],
    ));
    // A nested form uses its own /Tile name, overriding the parent's image.
    let tile = doc.add_object(Stream::new(dictionary! {
        "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(),0.into(),20.into(),20.into()],
        "Resources" => dictionary! {},
    }, b"0 0 1 rg -5 -5 30 30 re f 1 0 0 rg 3 3 5 5 re f".to_vec()));
    let nested = doc.add_object(Stream::new(dictionary! {
        "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(),0.into(),30.into(),25.into()],
        "Matrix" => vec![1.into(),0.2.into(),0.3.into(),1.into(),5.into(),5.into()],
        "Resources" => dictionary! {"XObject" => dictionary! {"Tile" => tile}},
    }, b"q 2 2 24 18 re 8 7 9 8 re W* n /Tile Do Q".to_vec()));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"\
        q 1 0 0 1 5 53 cm /Nested Do Q
        q 32 0 0 24 58 66 cm /Tile Do Q
        q -20 0 8 18 76 38 cm /Tile Do Q
        q 5 5 25 30 re W n 0 0.6 0 rg 0 8 m 8 45 30 -5 40 25 c 40 5 l h f Q
        q 38 5 20 22 re 43 10 8 10 re W* n 0 0 0 rg 30 0 40 35 re f Q
        q 70 5 20 20 re W n 0 0 0 RG 60 15 m 100 15 l S Q
        q 8 39 13 9 re 12 41 4 3 re W* n 20 0 0 12 5 38 cm /Tile Do Q
    "
        .to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(),0.into(),100.into(),100.into()], "Contents" => content,
        "Resources" => dictionary! {"XObject" => dictionary! {"Nested" => nested, "Tile" => image}},
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1},
        ),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    doc.trailer.set("Root", catalog);
    doc.compress();
    doc
}

fn render(artwork: &PdfArtwork) -> image::RgbImage {
    let color = |color: Option<PdfRgbColor>| {
        color.map_or("none".to_string(), |c| {
            format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
        })
    };
    let matrix = |t: beambench_common::Transform2D| {
        format!("matrix({} {} {} {} {} {})", t.a, t.b, t.c, t.d, t.tx, t.ty)
    };
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="400" height="400" viewBox="0 0 {} {}"><rect width="100%" height="100%" fill="white"/><g transform="{}">"#,
        100.0 * MM,
        100.0 * MM,
        matrix(artwork.page_to_canvas)
    );
    for path in &artwork.paths {
        svg.push_str(&format!(
            r#"<path d="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
            path.path.to_svg_d(),
            if matches!(
                path.paint_mode,
                PdfPaintMode::Fill | PdfPaintMode::FillStroke
            ) {
                color(path.fill_color)
            } else {
                "none".into()
            },
            if matches!(
                path.paint_mode,
                PdfPaintMode::Stroke | PdfPaintMode::FillStroke
            ) {
                color(path.stroke_color)
            } else {
                "none".into()
            },
            MM
        ));
    }
    for (index, image) in artwork.images.iter().enumerate() {
        if let Some(clip) = &image.clip {
            svg.push_str(&format!(r#"<defs><clipPath id="clip{index}"><path d="{}"/></clipPath></defs><g clip-path="url(#clip{index})">"#,clip.to_svg_d()));
        }
        svg.push_str(&format!(r#"<image width="1" height="1" preserveAspectRatio="none" image-rendering="optimizeSpeed" transform="{}" xlink:href="data:image/png;base64,{}"/>"#,matrix(image.transform),base64::engine::general_purpose::STANDARD.encode(&image.png)));
        if image.clip.is_some() {
            svg.push_str("</g>");
        }
    }
    svg.push_str("</g></svg>");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(400, 400).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    image::load_from_memory(&pixmap.encode_png().unwrap())
        .unwrap()
        .to_rgb8()
}

#[test]
fn generated_mixed_artwork_matches_independent_rendering() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf");
    if std::env::var_os("PDF_REGENERATE").is_some() {
        std::fs::create_dir_all(&directory).unwrap();
        let renderer = std::env::var_os("PDFTOPPM")
            .expect("PDFTOPPM must name an independent Poppler renderer");
        for modern in [false, true] {
            let stem = if modern {
                "mixed-modern"
            } else {
                "mixed-classic"
            };
            let path = directory.join(format!("{stem}.pdf"));
            let mut doc = fixture();
            let mut bytes = Vec::new();
            if modern {
                doc.save_modern(&mut bytes).unwrap();
            } else {
                doc.save_to(&mut bytes).unwrap();
            }
            std::fs::write(&path, bytes).unwrap();
            let status = std::process::Command::new(&renderer)
                .args([
                    "-png",
                    "-singlefile",
                    "-scale-to",
                    "400",
                    "-aa",
                    "no",
                    "-aaVector",
                    "no",
                ])
                .arg(&path)
                .arg(directory.join(stem))
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
    let mut first = None;
    for stem in ["mixed-classic", "mixed-modern"] {
        let bytes = std::fs::read(directory.join(format!("{stem}.pdf"))).unwrap();
        let artwork = parse_pdf_artwork(&bytes).unwrap();
        assert_eq!(artwork.images.len(), 3);
        if let Some(previous) = &first {
            assert_eq!(&artwork, previous);
        } else {
            first = Some(artwork.clone());
        }
        let actual = render(&artwork);
        let expected = image::open(directory.join(format!("{stem}.png")))
            .unwrap()
            .to_rgb8();
        assert_eq!(actual.dimensions(), expected.dimensions());
        // Different rasterizers disagree at antialiased boundaries. Compare
        // interiors only, excluding a one-pixel neighborhood of a reference edge.
        let mut mismatches = 0;
        let mut compared = 0;
        for y in 1..399 {
            for x in 1..399 {
                let pixel = expected.get_pixel(x, y).0;
                if (y - 1..=y + 1)
                    .any(|yy| (x - 1..=x + 1).any(|xx| expected.get_pixel(xx, yy).0 != pixel))
                {
                    continue;
                }
                compared += 1;
                if actual
                    .get_pixel(x, y)
                    .0
                    .iter()
                    .zip(pixel)
                    .any(|(a, b)| a.abs_diff(b) > 8)
                {
                    mismatches += 1;
                }
            }
        }
        assert!(compared > 140_000);
        if mismatches > 20 {
            actual
                .save(std::env::temp_dir().join(format!("beambench-{stem}-actual.png")))
                .unwrap();
        }
        assert!(
            mismatches <= 20,
            "{stem}: {mismatches}/{compared} interior pixels differ from Poppler"
        );
    }
}
