//! Thread-safe LRU caches for raster processing.
//!
//! Two cache tiers:
//! - `RasterCache` — caches final `ProcessedRaster` results (keyed by all params)
//! - `ScaledImageCache` — caches decoded+scaled grayscale images (keyed by source+geometry)
//!
//! Cache hits return `Arc` clones (ref-count bump, no pixel data copy).

use std::sync::{Arc, Mutex};

use crate::types::ProcessedRaster;

/// Generic thread-safe LRU cache. Proper LRU: get() promotes, evicts least-recently-used.
struct LruCache<T> {
    entries: Vec<(String, Arc<T>, usize)>,
    byte_budget: usize,
    bytes: usize,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl<T> LruCache<T> {
    fn new(capacity: usize, byte_budget: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
            byte_budget,
            bytes: 0,
            hits: 0,
            misses: 0,
        }
    }

    fn get(&mut self, key: &str) -> Option<Arc<T>> {
        if let Some(pos) = self.entries.iter().position(|(k, _, _)| k == key) {
            let entry = self.entries.remove(pos);
            let value = entry.1.clone();
            self.entries.push(entry);
            self.hits += 1;
            Some(value)
        } else {
            self.misses += 1;
            None
        }
    }

    fn insert(&mut self, key: String, value: Arc<T>, bytes: usize) {
        if let Some(pos) = self.entries.iter().position(|(k, _, _)| k == &key) {
            self.bytes -= self.entries.remove(pos).2;
        }
        // A single oversized image remains usable by its caller, but is not cached.
        if self.capacity == 0 || bytes > self.byte_budget {
            return;
        }
        while self.entries.len() >= self.capacity || self.bytes > self.byte_budget - bytes {
            self.bytes -= self.entries.remove(0).2;
        }
        self.bytes += bytes;
        self.entries.push((key, value, bytes));
    }

    fn hits(&self) -> u64 {
        self.hits
    }
    fn misses(&self) -> u64 {
        self.misses
    }
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Default byte budget for the processed-raster and scaled-image caches.
///
/// A single large grayscale raster is substantial on its own (a 5000x5000
/// `Grayscale8` image is about 25 MB), so a budget of a few tens of megabytes
/// cannot hold even the two or three images a normal project contains, and
/// every preview re-runs the whole decode/scale/dither pipeline from source.
/// 256 MiB keeps the common 1-3 large-image case resident while staying
/// bounded. `BEAMBENCH_RASTER_CACHE_MB` overrides it for constrained machines.
fn default_byte_budget() -> usize {
    const DEFAULT_MB: usize = 256;
    std::env::var("BEAMBENCH_RASTER_CACHE_MB")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|mb| *mb > 0)
        .unwrap_or(DEFAULT_MB)
        .saturating_mul(1024 * 1024)
}

/// Thread-safe LRU cache for final processed raster results.
pub struct RasterCache {
    inner: Mutex<LruCache<ProcessedRaster>>,
}

impl RasterCache {
    pub fn new(capacity: usize) -> Self {
        Self::with_byte_budget(capacity, default_byte_budget())
    }

    pub fn with_byte_budget(capacity: usize, byte_budget: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(capacity, byte_budget)),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ProcessedRaster>> {
        self.inner.lock().unwrap().get(key)
    }

    pub fn insert(&self, key: String, value: Arc<ProcessedRaster>) {
        let bytes = value.data.capacity();
        self.inner.lock().unwrap().insert(key, value, bytes);
    }

    pub fn retained_bytes(&self) -> usize {
        self.inner.lock().unwrap().bytes
    }

    pub fn hit_count(&self) -> u64 {
        self.inner.lock().unwrap().hits()
    }
    pub fn miss_count(&self) -> u64 {
        self.inner.lock().unwrap().misses()
    }
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().len() == 0
    }
}

/// Cached decoded+scaled grayscale image. This is the expensive part of
/// the pipeline (decode + saturation adjust + scale) that doesn't change
/// when brightness/contrast/gamma sliders move.
pub struct ScaledImage {
    pub image: image::GrayImage,
    pub target_w: u32,
    pub target_h: u32,
}

/// Thread-safe LRU cache for decoded+scaled grayscale images.
/// Keyed by source bytes hash + geometry (bounds, DPI, saturation).
pub struct ScaledImageCache {
    inner: Mutex<LruCache<ScaledImage>>,
}

impl ScaledImageCache {
    pub fn new(capacity: usize) -> Self {
        Self::with_byte_budget(capacity, default_byte_budget())
    }

    pub fn with_byte_budget(capacity: usize, byte_budget: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(capacity, byte_budget)),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ScaledImage>> {
        self.inner.lock().unwrap().get(key)
    }

    pub fn insert(&self, key: String, value: Arc<ScaledImage>) {
        let bytes = value.image.as_raw().capacity();
        self.inner.lock().unwrap().insert(key, value, bytes);
    }

    pub fn retained_bytes(&self) -> usize {
        self.inner.lock().unwrap().bytes
    }

    pub fn hit_count(&self) -> u64 {
        self.inner.lock().unwrap().hits()
    }
    pub fn miss_count(&self) -> u64 {
        self.inner.lock().unwrap().misses()
    }
}

/// Compute a cache key for the decode+scale stage.
/// Only includes fields that affect the decoded/scaled image:
/// source bytes, bounds, DPI, saturation, pass_through.
pub fn scaled_image_key(params: &crate::types::RasterProcessingParams) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&params.source_bytes);
    h.update(params.bounds_mm.0.to_le_bytes());
    h.update(params.bounds_mm.1.to_le_bytes());
    h.update(params.dpi.to_le_bytes());
    h.update(params.adjustments.saturation.to_le_bytes());
    h.update([params.pass_through as u8]);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ProcessedRaster, RasterPixelFormat};

    #[test]
    fn byte_budget_evicts_lru_and_skips_oversized_and_disabled_entries() {
        let cache = RasterCache::with_byte_budget(32, 200);
        cache.insert("a".into(), Arc::new(make_raster(1)));
        cache.insert("b".into(), Arc::new(make_raster(2)));
        let in_use = cache.get("a").unwrap();
        cache.insert("c".into(), Arc::new(make_raster(3)));
        assert!(cache.get("b").is_none());
        assert_eq!(in_use.data[0], 1);
        assert_eq!(cache.retained_bytes(), 200);
        let mut big = make_raster(4);
        big.data = vec![4; 201];
        cache.insert("c".into(), Arc::new(big));
        assert!(cache.get("c").is_none());
        assert_eq!(cache.retained_bytes(), 100);
        let disabled = RasterCache::with_byte_budget(0, 200);
        disabled.insert("a".into(), in_use);
        assert!(disabled.is_empty());
    }

    #[test]
    fn scaled_cache_counts_allocated_pixel_bytes() {
        let cache = ScaledImageCache::with_byte_budget(16, 100);
        for key in ["a", "b"] {
            cache.insert(
                key.into(),
                Arc::new(ScaledImage {
                    image: image::GrayImage::new(10, 10),
                    target_w: 10,
                    target_h: 10,
                }),
            );
        }
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert_eq!(cache.retained_bytes(), 100);
    }

    fn make_raster(tag: u8) -> ProcessedRaster {
        ProcessedRaster {
            width_px: 10,
            height_px: 10,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![tag; 100],
        }
    }

    #[test]
    fn get_returns_none_on_miss() {
        let cache = RasterCache::new(4);
        assert!(cache.get("nonexistent").is_none());
        assert_eq!(cache.miss_count(), 1);
        assert_eq!(cache.hit_count(), 0);
    }

    #[test]
    fn insert_then_get_returns_value() {
        let cache = RasterCache::new(4);
        cache.insert("k1".into(), Arc::new(make_raster(1)));
        let v = cache.get("k1");
        assert!(v.is_some());
        assert_eq!(v.unwrap().data[0], 1);
        assert_eq!(cache.hit_count(), 1);
    }

    #[test]
    fn lru_evicts_least_recently_used_not_oldest_insert() {
        let cache = RasterCache::new(3);
        cache.insert("a".into(), Arc::new(make_raster(1)));
        cache.insert("b".into(), Arc::new(make_raster(2)));
        cache.insert("c".into(), Arc::new(make_raster(3)));
        // Access "a" -- promotes it to MRU
        assert!(cache.get("a").is_some());
        // Insert "d" -- should evict "b" (LRU), not "a" (recently accessed)
        cache.insert("d".into(), Arc::new(make_raster(4)));
        assert!(
            cache.get("a").is_some(),
            "a should survive (recently accessed)"
        );
        assert!(cache.get("b").is_none(), "b should be evicted (LRU)");
        assert!(cache.get("c").is_some(), "c should survive");
        assert!(cache.get("d").is_some(), "d should exist (just inserted)");
    }

    #[test]
    fn insert_overwrites_existing_key() {
        let cache = RasterCache::new(4);
        cache.insert("k".into(), Arc::new(make_raster(1)));
        cache.insert("k".into(), Arc::new(make_raster(2)));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("k").unwrap().data[0], 2);
    }

    #[test]
    fn capacity_enforced() {
        let cache = RasterCache::new(2);
        cache.insert("a".into(), Arc::new(make_raster(1)));
        cache.insert("b".into(), Arc::new(make_raster(2)));
        cache.insert("c".into(), Arc::new(make_raster(3)));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("a").is_none(), "a should be evicted");
    }
}
