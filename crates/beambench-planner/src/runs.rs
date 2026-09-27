//! Raster rows retain pixels and expand burn runs only while a consumer visits the row.
//! The serialized form remains the existing array of ScanRun records.
use crate::plan::ScanRun;
use beambench_raster::{ProcessedRaster, RasterPixelFormat};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ScanRuns(Storage);

#[derive(Debug, Clone)]
enum Storage {
    Explicit(Arc<Vec<ScanRun>>),
    Pixels {
        raster: Arc<ProcessedRaster>,
        row: u32,
        origin: f64,
        count: usize,
        /// Burn pixels in this row, i.e. the `power_values` bytes expanded runs
        /// would carry. Counted during the same pass that counts the runs.
        burn_pixels: usize,
        /// Minimum and maximum X of every run endpoint in this row, in mm.
        /// Cached so extent queries never re-decode the row: the nearest-neighbor
        /// optimizer asks for it once per candidate comparison.
        extent: Option<(f64, f64)>,
        reversed: bool,
        transforms: Vec<Transform>,
    },
}

#[derive(Debug, Clone, Copy)]
enum Transform {
    Translate(f64),
    Reflect(f64),
    Trim(f64),
}

impl From<Vec<ScanRun>> for ScanRuns {
    fn from(runs: Vec<ScanRun>) -> Self {
        Self(Storage::Explicit(Arc::new(runs)))
    }
}
impl FromIterator<ScanRun> for ScanRuns {
    fn from_iter<T: IntoIterator<Item = ScanRun>>(iter: T) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}
impl IntoIterator for ScanRuns {
    type Item = ScanRun;
    type IntoIter = std::vec::IntoIter<ScanRun>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
            .map(Cow::into_owned)
            .collect::<Vec<_>>()
            .into_iter()
    }
}
impl Default for ScanRuns {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl PartialEq for ScanRuns {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}
impl Serialize for ScanRuns {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}
impl<'de> Deserialize<'de> for ScanRuns {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<ScanRun>::deserialize(deserializer).map(Into::into)
    }
}

pub enum RunIter<'a> {
    Explicit(std::slice::Iter<'a, ScanRun>),
    Pixels(std::vec::IntoIter<ScanRun>),
}
impl<'a> Iterator for RunIter<'a> {
    type Item = Cow<'a, ScanRun>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Explicit(it) => it.next().map(Cow::Borrowed),
            Self::Pixels(it) => it.next().map(Cow::Owned),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Explicit(it) => it.size_hint(),
            Self::Pixels(it) => it.size_hint(),
        }
    }
}
impl DoubleEndedIterator for RunIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Explicit(it) => it.next_back().map(Cow::Borrowed),
            Self::Pixels(it) => it.next_back().map(Cow::Owned),
        }
    }
}
impl ExactSizeIterator for RunIter<'_> {}
impl<'a> IntoIterator for &'a ScanRuns {
    type Item = Cow<'a, ScanRun>;
    type IntoIter = RunIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl ScanRuns {
    pub(crate) fn from_pixels(raster: Arc<ProcessedRaster>, row: u32, origin: f64) -> Self {
        let width = raster.width_px as usize;
        let stride = match raster.format {
            RasterPixelFormat::Binary => width.div_ceil(8),
            RasterPixelFormat::Grayscale8 => width,
        };
        let x_step = raster.effective_x_pixel_mm();
        let mut in_run = false;
        let mut count = 0;
        let mut burn_pixels = 0usize;
        let mut first_burn: Option<usize> = None;
        let mut last_end = 0usize;
        let mut truncated_open = false;
        for x in 0..width {
            let index = match raster.format {
                RasterPixelFormat::Binary => row as usize * stride + x / 8,
                RasterPixelFormat::Grayscale8 => row as usize * stride + x,
            };
            // `find_binary_runs`/`find_grayscale_runs` stop at the end of the
            // backing buffer and close any open run at the row's full width.
            // Mirror that here so the cached extent matches what `iter()` yields.
            let Some(byte) = raster.data.get(index) else {
                truncated_open = in_run;
                break;
            };
            let burn = match raster.format {
                RasterPixelFormat::Binary => byte & (1 << (7 - x % 8)) == 0,
                RasterPixelFormat::Grayscale8 => *byte < 255,
            };
            if burn {
                if !in_run {
                    count += 1;
                    first_burn.get_or_insert(x);
                }
                burn_pixels += 1;
                last_end = x + 1;
            }
            in_run = burn;
        }
        if truncated_open {
            last_end = width;
        }
        let extent = first_burn
            .map(|start| (origin + start as f64 * x_step, origin + last_end as f64 * x_step));
        Self(Storage::Pixels {
            raster,
            row,
            origin,
            count,
            burn_pixels,
            extent,
            reversed: false,
            transforms: Vec::new(),
        })
    }
    /// Identity and byte size of the bitmap this row reads, when pixel-backed.
    ///
    /// Every row of one pass shares a single allocation, so the budget charges
    /// it once per distinct bitmap rather than once per row or per angle pass.
    pub(crate) fn retained_bitmap(&self) -> Option<(usize, usize)> {
        match &self.0 {
            Storage::Pixels { raster, .. } => {
                Some((Arc::as_ptr(raster) as usize, raster.data.len()))
            }
            Storage::Explicit(_) => None,
        }
    }

    /// Bytes this row retains, or would retain, holding expanded runs.
    pub(crate) fn expanded_bytes(&self) -> usize {
        const RUN: usize = std::mem::size_of::<ScanRun>();
        match &self.0 {
            Storage::Explicit(runs) => {
                runs.len() * RUN + runs.iter().map(|run| run.power_values.len()).sum::<usize>()
            }
            Storage::Pixels {
                count,
                burn_pixels,
                raster,
                ..
            } => {
                count * RUN
                    + match raster.format {
                        // Binary runs carry no per-pixel powers.
                        RasterPixelFormat::Binary => 0,
                        RasterPixelFormat::Grayscale8 => *burn_pixels,
                    }
            }
        }
    }

    /// Expand pixel-backed storage into explicit runs, releasing the bitmap.
    pub(crate) fn materialize(&mut self) {
        if matches!(self.0, Storage::Pixels { .. }) {
            self.0 = Storage::Explicit(Arc::new(self.iter().map(Cow::into_owned).collect()));
        }
    }

    pub fn len(&self) -> usize {
        match &self.0 {
            Storage::Explicit(runs) => runs.len(),
            Storage::Pixels { count, .. } => *count,
        }
    }
    /// Lowest and highest X coordinate touched by this row, in mm.
    ///
    /// Pixel-backed rows answer from a cached value rather than decoding, so
    /// hot callers (motion points, travel ordering, stats, validation) stay
    /// O(1) per row instead of re-expanding every run.
    pub fn x_extent(&self) -> Option<(f64, f64)> {
        match &self.0 {
            Storage::Explicit(runs) => explicit_extent(runs),
            Storage::Pixels { extent, .. } => *extent,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn iter(&self) -> RunIter<'_> {
        match &self.0 {
            Storage::Explicit(runs) => RunIter::Explicit(runs.iter()),
            Storage::Pixels {
                raster,
                row,
                origin,
                reversed,
                transforms,
                ..
            } => {
                let mut runs = match raster.format {
                    RasterPixelFormat::Binary => {
                        crate::scanline::find_binary_runs(raster, *row, *origin, 0.0)
                    }
                    RasterPixelFormat::Grayscale8 => {
                        crate::scanline::find_grayscale_runs(raster, *row, *origin, 0.0)
                    }
                };
                for transform in transforms {
                    apply(&mut runs, *transform);
                }
                if *reversed {
                    runs.reverse();
                }
                RunIter::Pixels(runs.into_iter())
            }
        }
    }
    pub fn get(&self, index: usize) -> Option<Cow<'_, ScanRun>> {
        self.iter().nth(index)
    }
    pub fn first(&self) -> Option<Cow<'_, ScanRun>> {
        self.iter().next()
    }
    pub fn last(&self) -> Option<Cow<'_, ScanRun>> {
        self.iter().next_back()
    }
    pub fn reverse(&mut self) {
        match &mut self.0 {
            Storage::Explicit(runs) => Arc::make_mut(runs).reverse(),
            Storage::Pixels { reversed, .. } => *reversed = !*reversed,
        }
    }
    pub fn translate(&mut self, offset: f64) {
        self.transform(Transform::Translate(offset));
    }
    pub fn reflect(&mut self, origin: f64) {
        self.transform(Transform::Reflect(origin));
    }
    pub fn trim(&mut self, amount: f64) {
        self.transform(Transform::Trim(amount));
    }
    fn transform(&mut self, transform: Transform) {
        match &mut self.0 {
            Storage::Explicit(runs) => apply(Arc::make_mut(runs), transform),
            Storage::Pixels {
                transforms, extent, ..
            } => {
                transforms.push(transform);
                // Translate and Reflect move every endpoint by a closed-form
                // rule, so the cached extent follows without decoding.
                match transform {
                    Transform::Translate(dx) => {
                        if let Some((lo, hi)) = extent {
                            *lo += dx;
                            *hi += dx;
                        }
                    }
                    Transform::Reflect(origin) => {
                        if let Some((lo, hi)) = extent {
                            let (a, b) = (origin - *hi, origin - *lo);
                            *lo = a;
                            *hi = b;
                        }
                    }
                    // Trim can delete the outermost runs entirely, so the new
                    // extent is not derivable from the old one.
                    Transform::Trim(_) => {}
                }
            }
        }
        if matches!(transform, Transform::Trim(_)) {
            // One decode recomputes both the surviving run count and extent.
            let expanded: Vec<ScanRun> = self.iter().map(Cow::into_owned).collect();
            let (len, bounds) = (expanded.len(), explicit_extent(&expanded));
            if let Storage::Pixels { count, extent, .. } = &mut self.0 {
                *count = len;
                *extent = bounds;
            }
        }
    }
    /// Used by callers that deliberately edit individual runs, such as quality-test artwork.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, ScanRun> {
        self.materialize();
        match &mut self.0 {
            Storage::Explicit(runs) => Arc::make_mut(runs).iter_mut(),
            _ => unreachable!(),
        }
    }
}
fn explicit_extent(runs: &[ScanRun]) -> Option<(f64, f64)> {
    runs.iter().fold(None, |acc: Option<(f64, f64)>, run| {
        let lo = run.start_x_mm.min(run.end_x_mm);
        let hi = run.start_x_mm.max(run.end_x_mm);
        Some(match acc {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        })
    })
}

fn apply(runs: &mut Vec<ScanRun>, transform: Transform) {
    runs.retain_mut(|run| {
        match transform {
            Transform::Translate(dx) => {
                run.start_x_mm += dx;
                run.end_x_mm += dx;
            }
            Transform::Reflect(origin) => {
                run.start_x_mm = origin - run.start_x_mm;
                run.end_x_mm = origin - run.end_x_mm;
            }
            Transform::Trim(trim) => {
                run.start_x_mm += trim;
                run.end_x_mm -= trim;
                return run.end_x_mm - run.start_x_mm > 1e-9;
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixel_rows_match_expanded_rows_after_transforms_and_round_trip() {
        for format in [RasterPixelFormat::Binary, RasterPixelFormat::Grayscale8] {
            for width in [1u32, 7, 8, 13, 255] {
                let stride = if format == RasterPixelFormat::Binary {
                    width.div_ceil(8)
                } else {
                    width
                };
                let mut state = 42u32;
                let data = (0..stride * 5)
                    .map(|_| {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        if state % 3 == 0 {
                            255
                        } else {
                            (state >> 24) as u8
                        }
                    })
                    .collect();
                let raster = Arc::new(ProcessedRaster {
                    width_px: width,
                    height_px: 5,
                    line_interval_mm: 0.13,
                    x_pixel_mm: 0.07,
                    format,
                    data,
                });
                for row in 0..5 {
                    let mut compact = ScanRuns::from_pixels(raster.clone(), row, 19.1234);
                    assert_eq!(
                        compact.x_extent(),
                        explicit_extent(
                            &compact.iter().map(Cow::into_owned).collect::<Vec<_>>()
                        )
                    );
                    let original = compact.clone();
                    let expanded = match format {
                        RasterPixelFormat::Binary => {
                            crate::scanline::find_binary_runs(&raster, row, 19.1234, 0.0)
                        }
                        RasterPixelFormat::Grayscale8 => {
                            crate::scanline::find_grayscale_runs(&raster, row, 19.1234, 0.0)
                        }
                    };
                    let mut explicit: ScanRuns = expanded.into();
                    for transform in [
                        Transform::Trim(0.01),
                        Transform::Translate(11.6789),
                        Transform::Reflect(400.0),
                        Transform::Translate(-53.0),
                    ] {
                        compact.transform(transform);
                        explicit.transform(transform);
                    }
                    compact.reverse();
                    explicit.reverse();
                    assert_eq!(compact, explicit);
                    assert_eq!(compact.len(), compact.iter().count());
                    assert_eq!(compact.x_extent(), explicit.x_extent());
                    assert_eq!(
                        compact.x_extent(),
                        explicit_extent(
                            &compact.iter().map(Cow::into_owned).collect::<Vec<_>>()
                        )
                    );
                    let json = serde_json::to_string(&compact).unwrap();
                    assert_eq!(json, serde_json::to_string(&explicit).unwrap());
                    assert_eq!(compact, serde_json::from_str::<ScanRuns>(&json).unwrap());
                    assert_eq!(
                        original,
                        ScanRuns::from_pixels(raster.clone(), row, 19.1234)
                    );
                    assert!(matches!(compact.0, Storage::Pixels { .. }));
                }
            }
        }
    }
    #[test]
    fn trimming_all_burn_runs_preserves_empty_row_semantics() {
        let raster = Arc::new(ProcessedRaster {
            width_px: 8,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xAA],
        });
        let mut row = ScanRuns::from_pixels(raster, 0, 0.0);
        assert!(row.x_extent().is_some());
        row.trim(0.06);
        assert!(row.is_empty());
        assert_eq!(row.iter().count(), 0);
        assert_eq!(row.x_extent(), None);
    }
}
