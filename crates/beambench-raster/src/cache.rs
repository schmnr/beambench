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
    entries: Vec<(String, Arc<T>)>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl<T> LruCache<T> {
    fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    fn get(&mut self, key: &str) -> Option<Arc<T>> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
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

    fn insert(&mut self, key: String, value: Arc<T>) {
        self.entries.retain(|(k, _)| k != &key);
        while self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((key, value));
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

/// Thread-safe LRU cache for final processed raster results.
pub struct RasterCache {
    inner: Mutex<LruCache<ProcessedRaster>>,
}

impl RasterCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ProcessedRaster>> {
        self.inner.lock().unwrap().get(key)
    }

    pub fn insert(&self, key: String, value: Arc<ProcessedRaster>) {
        self.inner.lock().unwrap().insert(key, value);
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
        Self {
            inner: Mutex::new(LruCache::new(capacity)),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ScaledImage>> {
        self.inner.lock().unwrap().get(key)
    }

    pub fn insert(&self, key: String, value: Arc<ScaledImage>) {
        self.inner.lock().unwrap().insert(key, value);
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
