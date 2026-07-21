use barcoders::sym::codabar::Codabar;
use barcoders::sym::code39::Code39;
use barcoders::sym::code93::Code93;
use barcoders::sym::code128::Code128;
use barcoders::sym::ean8::EAN8;
use barcoders::sym::ean13::{EAN13, UPCA};
use barcoders::sym::tf::TF;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use beambench_common::{BarcodeOptions, BarcodeType, QrErrorCorrection};
use datamatrix::{DataMatrix, SymbolList};
use pdf417::{pdf417_height, pdf417_width};
use qrcodegen::{QrCode, QrCodeEcc};

const CODE128_START_B: char = 'Ɓ';

pub fn generate_barcode(
    barcode_type: BarcodeType,
    data: &str,
    width: f64,
    height: f64,
) -> Result<VecPath, String> {
    generate_barcode_with_options(
        barcode_type,
        data,
        width,
        height,
        &BarcodeOptions::default(),
    )
}

pub fn generate_barcode_with_options(
    barcode_type: BarcodeType,
    data: &str,
    width: f64,
    height: f64,
    options: &BarcodeOptions,
) -> Result<VecPath, String> {
    if data.is_empty() {
        return Err("Barcode data cannot be empty".to_string());
    }
    if width <= 0.0 || height <= 0.0 {
        return Err("Barcode dimensions must be positive".to_string());
    }

    match barcode_type {
        BarcodeType::Code128 => {
            let mut encoded = String::with_capacity(data.len() + 1);
            encoded.push(CODE128_START_B);
            encoded.push_str(data);
            let bits = Code128::new(&encoded)
                .map_err(|err| format!("invalid Code128 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Code39 => {
            let bits = Code39::new(data)
                .map_err(|err| format!("invalid Code39 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Code93 => {
            let bits = Code93::new(data)
                .map_err(|err| format!("invalid Code93 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Codabar => {
            let bits = Codabar::new(data)
                .map_err(|err| format!("invalid CodaBar data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Standard2Of5 => {
            let bits = TF::standard(data)
                .map_err(|err| format!("invalid Standard 2 of 5 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Ean13 => {
            let bits = EAN13::new(data)
                .map_err(|err| format!("invalid EAN-13 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::Ean8 => {
            let bits = EAN8::new(data)
                .map_err(|err| format!("invalid EAN-8 data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::UpcA => {
            let bits = UPCA::new(data)
                .map_err(|err| format!("invalid UPC-A data: {err}"))?
                .encode();
            linear_modules_to_path(&bits, width, height)
        }
        BarcodeType::QrCode => {
            let code = QrCode::encode_text(data, qr_ecc(options.qr_error_correction))
                .map_err(|err| format!("invalid QR code data: {err}"))?;
            let size = code.size() as usize;
            let mut cells = Vec::new();
            for y in 0..size {
                for x in 0..size {
                    if code.get_module(x as i32, y as i32) {
                        cells.push((x, y));
                    }
                }
            }
            matrix_modules_to_path(size, size, &cells, width, height)
        }
        BarcodeType::DataMatrix => {
            let symbols = if options.data_matrix_force_square {
                SymbolList::default().enforce_square()
            } else {
                SymbolList::default()
            };
            let code = DataMatrix::encode_str(data, symbols)
                .map_err(|err| format!("invalid Data Matrix data: {err:?}"))?;
            let bitmap = code.bitmap();
            let width_modules = bitmap.width();
            let height_modules = bitmap.height();
            let cells: Vec<(usize, usize)> = bitmap.pixels().collect();
            matrix_modules_to_path(width_modules, height_modules, &cells, width, height)
        }
        BarcodeType::Pdf417 => generate_pdf417(data, width, height),
    }
}

fn qr_ecc(level: QrErrorCorrection) -> QrCodeEcc {
    match level {
        QrErrorCorrection::Low => QrCodeEcc::Low,
        QrErrorCorrection::Medium => QrCodeEcc::Medium,
        QrErrorCorrection::Quartile => QrCodeEcc::Quartile,
        QrErrorCorrection::High => QrCodeEcc::High,
    }
}

fn generate_pdf417(data: &str, width: f64, height: f64) -> Result<VecPath, String> {
    let used = pdf417_used_codewords(data)?;
    let target_ratio = width / height;
    let (rows, cols) = choose_pdf417_dimensions(used + 2, target_ratio)
        .ok_or_else(|| "input is too large for PDF417".to_string())?;

    let capacity = rows as usize * cols as usize;
    let mut codewords = vec![0u16; capacity];
    let encoder = pdf417::PDF417Encoder::new(&mut codewords, false);
    let encoder = if data.is_ascii() {
        encoder.append_ascii(data)
    } else {
        encoder.append_utf8(data)
    };
    let (level, codewords) = encoder
        .fit_seal()
        .ok_or_else(|| "input is too large for PDF417".to_string())?;

    let rendered_width = pdf417::pdf417_width!(cols);
    let rendered_height = pdf417::pdf417_height!(rows);
    let mut pixels = vec![false; rendered_width * rendered_height];
    pdf417::PDF417::new(codewords, rows, cols, level).render(&mut pixels[..]);

    let mut cells = Vec::new();
    for y in 0..rendered_height {
        for x in 0..rendered_width {
            if pixels[y * rendered_width + x] {
                cells.push((x, y));
            }
        }
    }

    matrix_modules_to_path(rendered_width, rendered_height, &cells, width, height)
}

fn pdf417_used_codewords(data: &str) -> Result<usize, String> {
    let mut scratch = vec![0u16; (pdf417::MAX_ROWS as usize) * (pdf417::MAX_COLS as usize)];
    let encoder = pdf417::PDF417Encoder::new(&mut scratch, false);
    let encoder = if data.is_ascii() {
        encoder.append_ascii(data)
    } else {
        encoder.append_utf8(data)
    };
    Ok(encoder.count())
}

fn choose_pdf417_dimensions(required_capacity: usize, target_ratio: f64) -> Option<(u8, u8)> {
    let mut best: Option<(u8, u8, f64, usize)> = None;

    for rows in pdf417::MIN_ROWS..=pdf417::MAX_ROWS {
        for cols in pdf417::MIN_COLS..=pdf417::MAX_COLS {
            let capacity = rows as usize * cols as usize;
            if capacity < required_capacity {
                continue;
            }

            let rendered_width = pdf417::pdf417_width!(cols) as f64;
            let rendered_height = pdf417::pdf417_height!(rows) as f64;
            let aspect_error = ((rendered_width / rendered_height) - target_ratio).abs();

            match best {
                None => best = Some((rows, cols, aspect_error, capacity)),
                Some((_, _, best_error, best_capacity)) => {
                    if aspect_error < best_error - f64::EPSILON
                        || ((aspect_error - best_error).abs() <= f64::EPSILON
                            && capacity < best_capacity)
                    {
                        best = Some((rows, cols, aspect_error, capacity));
                    }
                }
            }
        }
    }

    best.map(|(rows, cols, _, _)| (rows, cols))
}

fn linear_modules_to_path(bits: &[u8], width: f64, height: f64) -> Result<VecPath, String> {
    let black_modules: Vec<(usize, usize)> = bits
        .iter()
        .enumerate()
        .filter_map(|(x, bit)| (*bit != 0).then_some((x, 0)))
        .collect();
    matrix_modules_to_path(bits.len(), 1, &black_modules, width, height)
}

fn matrix_modules_to_path(
    modules_w: usize,
    modules_h: usize,
    black_modules: &[(usize, usize)],
    width: f64,
    height: f64,
) -> Result<VecPath, String> {
    if modules_w == 0 || modules_h == 0 {
        return Err("barcode encoder produced empty output".to_string());
    }

    let scale = (width / modules_w as f64).min(height / modules_h as f64);
    let scaled_w = modules_w as f64 * scale;
    let scaled_h = modules_h as f64 * scale;
    let offset_x = (width - scaled_w) / 2.0;
    let offset_y = (height - scaled_h) / 2.0;

    let subpaths = black_modules
        .iter()
        .map(|(x, y)| {
            rectangle(
                offset_x + *x as f64 * scale,
                offset_y + *y as f64 * scale,
                scale,
                scale,
            )
        })
        .collect();

    Ok(VecPath { subpaths })
}

fn rectangle(x: f64, y: f64, width: f64, height: f64) -> SubPath {
    SubPath {
        commands: vec![
            PathCommand::MoveTo { x, y },
            PathCommand::LineTo { x: x + width, y },
            PathCommand::LineTo {
                x: x + width,
                y: y + height,
            },
            PathCommand::LineTo { x, y: y + height },
            PathCommand::Close,
        ],
        closed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fits(path: &VecPath, width: f64, height: f64) {
        let bounds = path.bounds().expect("expected barcode bounds");
        assert!(
            bounds.width() <= width + 1e-6,
            "width {} > {}",
            bounds.width(),
            width
        );
        assert!(
            bounds.height() <= height + 1e-6,
            "height {} > {}",
            bounds.height(),
            height
        );
    }

    #[test]
    fn generates_every_supported_barcode_type() {
        let cases = [
            (BarcodeType::Code128, "Hello-123"),
            (BarcodeType::Code39, "HELLO-123"),
            (BarcodeType::Code93, "HELLO-123"),
            (BarcodeType::Codabar, "A12345B"),
            (BarcodeType::Standard2Of5, "123456"),
            (BarcodeType::Ean13, "590123412345"),
            (BarcodeType::Ean8, "5512345"),
            (BarcodeType::UpcA, "012345612345"),
            (BarcodeType::QrCode, "Hello QR"),
            (BarcodeType::DataMatrix, "Hello DM"),
            (BarcodeType::Pdf417, "Hello PDF417"),
        ];

        for (barcode_type, data) in cases {
            let path = generate_barcode(barcode_type, data, 120.0, 80.0).unwrap();
            assert!(!path.is_empty(), "expected geometry for {:?}", barcode_type);
            assert_fits(&path, 120.0, 80.0);
        }
    }

    #[test]
    fn rejects_invalid_symbology_data() {
        let ean = generate_barcode(BarcodeType::Ean13, "ABC", 100.0, 50.0);
        assert!(ean.is_err());

        let upc = generate_barcode(BarcodeType::UpcA, "123", 100.0, 50.0);
        assert!(upc.is_err());
    }

    #[test]
    fn rejects_empty_data() {
        let err = generate_barcode(BarcodeType::Code128, "", 100.0, 50.0).unwrap_err();
        assert_eq!(err, "Barcode data cannot be empty");
    }

    #[test]
    fn qr_code_produces_many_cells() {
        let path = generate_barcode(BarcodeType::QrCode, "1234", 100.0, 100.0).unwrap();
        assert!(path.subpaths.len() > 16);
        assert_fits(&path, 100.0, 100.0);
    }

    #[test]
    fn pdf417_generation_produces_rectangular_symbol() {
        let path =
            generate_barcode(BarcodeType::Pdf417, "PDF417 test payload", 180.0, 80.0).unwrap();
        let bounds = path.bounds().unwrap();
        assert!(bounds.width() > bounds.height());
        assert_fits(&path, 180.0, 80.0);
    }
}
