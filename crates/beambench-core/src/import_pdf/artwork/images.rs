use super::*;
use image::{ImageEncoder, ImageFormat, ImageReader, Rgba, RgbaImage};
use std::io::Cursor;

pub(super) fn decode_image(
    stream: &Stream,
    state: &State,
    budget: &mut Budget,
) -> Result<Option<PdfImage>, String> {
    let dict = &stream.dict;
    if dict
        .get(b"ImageMask")
        .is_ok_and(|v| !matches!(v.as_bool(), Ok(false)))
        || [
            b"Mask".as_slice(),
            b"SMask",
            b"SMaskInData",
            b"Alternates",
            b"OPI",
        ]
        .iter()
        .any(|key| dict.has(key))
    {
        return Err(
            "PDF image masks or compositing cannot be preserved; flatten the image first".into(),
        );
    }
    let dimension = |key| {
        dict.get(key)
            .and_then(Object::as_i64)
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| "Invalid PDF image dimensions".to_string())
    };
    let width = dimension(b"Width")?;
    let height = dimension(b"Height")?;
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or("PDF image pixel limit exceeded")?;
    charge(&mut budget.pixels, pixels, "image pixel")?;
    let bits = dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .map_err(|e| e.to_string())?;
    let channels = match dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .map_err(|_| "PDF image requires a DeviceGray, DeviceRGB or DeviceCMYK color space")?
    {
        b"DeviceGray" => 1,
        b"DeviceRGB" => 3,
        b"DeviceCMYK" => 4,
        _ => {
            return Err(
                "PDF image color space cannot be preserved; convert the image to RGB first".into(),
            );
        }
    };
    if bits != 8 && !(bits == 1 && channels == 1) {
        return Err("PDF images support 8-bit device colors or 1-bit grayscale".into());
    }
    let decode = match dict.get(b"Decode") {
        Ok(value) => numbers(value.as_array().map_err(|e| e.to_string())?, channels * 2)?,
        Err(_) => (0..channels).flat_map(|_| [0.0, 1.0]).collect(),
    };
    if decode.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Err("PDF image Decode values must be between zero and one".into());
    }
    // lopdf supports PNG predictors for 8-bit channels. Reject layouts it
    // otherwise silently ignores, including TIFF prediction and parameter arrays.
    if let Ok(params) = dict.get(b"DecodeParms")
        && !matches!(params, Object::Null)
    {
        let params = params
            .as_dict()
            .map_err(|_| "PDF image filter parameter arrays are unsupported")?;
        let integer = |key, default| -> Result<i64, String> {
            match params.get(key) {
                Ok(v) => v.as_i64().map_err(|e| e.to_string()),
                Err(_) => Ok(default),
            }
        };
        let predictor = integer(b"Predictor", 1)?;
        if predictor != 1
            && (!(10..=15).contains(&predictor)
                || bits != 8
                || integer(b"Columns", 1)? != i64::from(width)
                || integer(b"Colors", 1)? != channels as i64
                || integer(b"BitsPerComponent", 8)? != bits)
        {
            return Err("PDF image predictor cannot be preserved; export with PNG prediction or no predictor".into());
        }
    }
    let filters = if dict.has(b"Filter") {
        stream.filters().map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    let rgba = if filters == [b"DCTDecode".as_slice()] {
        if bits != 8
            || channels == 4
            || decode.chunks_exact(2).any(|p| p != [0.0, 1.0])
            || dict
                .get(b"DecodeParms")
                .is_ok_and(|p| !matches!(p, Object::Null))
        {
            return Err(
                "PDF JPEG requires 8-bit RGB or grayscale with default Decode values".into(),
            );
        }
        let reader = || ImageReader::with_format(Cursor::new(&stream.content), ImageFormat::Jpeg);
        if reader()
            .into_dimensions()
            .map_err(|e| format!("Invalid PDF JPEG: {e}"))?
            != (width, height)
        {
            return Err("PDF JPEG dimensions do not match its image dictionary".into());
        }
        // JPEG working memory is separate from the aggregate decoded-content
        // budget; image dimensions and total output pixels are bounded first.
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(width);
        limits.max_image_height = Some(height);
        limits.max_alloc = Some(128 * 1024 * 1024);
        let mut reader = reader();
        reader.limits(limits);
        charge(&mut budget.bytes, pixels * channels, "expanded content")?;
        reader
            .decode()
            .map_err(|e| format!("Cannot decode PDF JPEG: {e}"))?
            .to_rgba8()
    } else {
        let bytes = stream
            .get_plain_content_with_limit(budget.bytes)
            .map_err(|e| format!("Cannot decode PDF image within the content limit: {e}"))?;
        charge(&mut budget.bytes, bytes.len(), "expanded content")?;
        let row_bytes = (width as usize * channels * bits as usize).div_ceil(8);
        if bytes.len() != row_bytes * height as usize {
            return Err("PDF image byte count does not match its dimensions".into());
        }
        let mut rgba = RgbaImage::new(width, height);
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            let offset = y as usize * row_bytes + x as usize * channels;
            let mut values = [0.0; 4];
            for channel in 0..channels {
                let sample = if bits == 1 {
                    f64::from((bytes[y as usize * row_bytes + x as usize / 8] >> (7 - x % 8)) & 1)
                } else {
                    f64::from(bytes[offset + channel]) / 255.0
                };
                values[channel] =
                    decode[channel * 2] + sample * (decode[channel * 2 + 1] - decode[channel * 2]);
            }
            let color = match channels {
                1 => gray_to_rgb(values[0]),
                3 => device_rgb_to_rgb(values[0], values[1], values[2]),
                _ => device_cmyk_to_rgb(values[0], values[1], values[2], values[3]),
            };
            *pixel = Rgba([color.r, color.g, color.b, 255]);
        }
        rgba
    };
    let transform = state.graphics.ctm.compose(&Transform2D {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        tx: 0.0,
        ty: 1.0,
    });
    if transform.inverse().is_none() {
        return Err("PDF image has a singular transform".into());
    }
    let footprint = Polygon::new(
        LineString(
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)]
                .into_iter()
                .map(|(x, y)| {
                    let p = transform.apply(&Point2D::new(x, y));
                    Coord { x: p.x, y: p.y }
                })
                .collect(),
        ),
        vec![],
    );
    let clip = if let Some(clip) = &state.clip
        && !clip.contains(&footprint)
    {
        let footprint = MultiPolygon(vec![footprint]);
        check_clipping_complexity(&[clip, &footprint])?;
        let clipped = clip.intersection(&footprint);
        if clipped.0.is_empty() {
            return Ok(None);
        }
        let path = line_path(
            clipped.0.into_iter().flat_map(|p| {
                let (outer, inner) = p.into_inner();
                std::iter::once(outer).chain(inner)
            }),
            true,
        );
        charge(
            &mut budget.points,
            path.command_count(),
            "image clipping geometry",
        )?;
        Some(path)
    } else {
        None
    };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            rgba.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Cannot encode imported PDF image: {e}"))?;
    Ok(Some(PdfImage {
        png,
        transform,
        clip,
    }))
}
