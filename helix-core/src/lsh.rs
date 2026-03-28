use crate::HelixError;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::HashMap;
use xxhash_rust::xxh32::xxh32;

pub type DocId = u64;

#[derive(Clone)]
pub struct LshConfig {
    pub bands: usize,          // default 24
    pub rows_per_band: usize,  // default 4 (kept for future bit-chunking extensions)
    pub hamming_threshold: u8, // default 3
}

impl LshConfig {
    /// Validates LSH configuration.
    ///
    /// # Errors
    /// Returns an error if configuration values are out of bounds.
    pub fn validate(&self) -> Result<(), HelixError> {
        if !(1..=128).contains(&self.bands) {
            return Err(HelixError::InvalidConfig("bands must be 1..=128".into()));
        }
        if self.hamming_threshold > 32 {
            return Err(HelixError::InvalidConfig(
                "hamming_threshold must be <= 32".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ReverseEntry {
    pub fingerprint: u64,
    pub url_hash: String,
}

pub enum DedupStatus {
    New,
    NearDuplicate {
        best_match: DocId,
        hamming_distance: u8,
    },
    ExactDuplicate {
        existing: DocId,
    },
}

pub struct LshIndex {
    bands: Vec<RwLock<HashMap<u32, SmallVec<[DocId; 4]>>>>,
    reverse: RwLock<HashMap<DocId, ReverseEntry>>,
    config: LshConfig,
}

impl LshIndex {
    /// Creates a new LSH index.
    ///
    /// # Errors
    /// Returns an error if the provided config is invalid.
    pub fn new(config: LshConfig) -> Result<Self, HelixError> {
        config.validate()?;

        let mut band_vec = Vec::with_capacity(config.bands);
        for _ in 0..config.bands {
            band_vec.push(RwLock::new(HashMap::new()));
        }

        Ok(Self {
            bands: band_vec,
            reverse: RwLock::new(HashMap::new()),
            config,
        })
    }

    fn band_value(fp: u64, band_idx: usize) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        let rotated = fp.rotate_left((band_idx * 3) as u32);

        #[allow(clippy::cast_possible_truncation)]
        xxh32(&rotated.to_le_bytes(), band_idx as u32)
    }

    /// Inserts a document into the index.
    ///
    /// # Errors
    /// Returns an error if the document ID already exists.
    pub fn insert(
        &self,
        doc_id: DocId,
        fingerprint: u64,
        url_hash: String,
    ) -> Result<(), HelixError> {
        // Write reverse map first
        {
            let mut rev = self.reverse.write();
            if rev.contains_key(&doc_id) {
                return Err(HelixError::DuplicateDocId(doc_id));
            }
            rev.insert(
                doc_id,
                ReverseEntry {
                    fingerprint,
                    url_hash,
                },
            );
        }

        // Then insert into bands
        for (band_idx, band_lock) in self.bands.iter().enumerate() {
            let key = Self::band_value(fingerprint, band_idx);
            let mut band = band_lock.write();
            let bucket = band.entry(key).or_insert_with(SmallVec::new);
            bucket.push(doc_id);
        }
        Ok(())
    }

    /// Deletes a document from the index.
    ///
    /// # Errors
    /// This function is idempotent but may return an error if internal state is corrupted.
    pub fn delete(&self, doc_id: DocId) -> Result<(), HelixError> {
        let entry = {
            let mut rev = self.reverse.write();
            match rev.remove(&doc_id) {
                Some(e) => e,
                None => return Ok(()), // idempotent
            }
        };

        // Clean bands using stored fingerprint
        for (band_idx, band_lock) in self.bands.iter().enumerate() {
            let key = Self::band_value(entry.fingerprint, band_idx);
            let mut band = band_lock.write();
            if let Some(bucket) = band.get_mut(&key) {
                bucket.retain(|id| *id != doc_id);
                if bucket.is_empty() {
                    band.remove(&key);
                }
            }
        }
        Ok(())
    }

    /// Updates an existing document.
    ///
    /// # Errors
    /// Returns an error if insertion fails or state is inconsistent.
    pub fn update(
        &self,
        doc_id: DocId,
        fingerprint: u64,
        url_hash: String,
    ) -> Result<(), HelixError> {
        self.delete(doc_id)?;
        self.insert(doc_id, fingerprint, url_hash)
    }

    /// Queries for near-duplicates.
    ///
    /// # Errors
    /// Returns an error if the index is not initialized.
    pub fn query(&self, fingerprint: u64) -> Result<DedupStatus, HelixError> {
        if self.bands.is_empty() {
            return Err(HelixError::IndexNotInitialized);
        }

        let mut candidates = std::collections::HashSet::new();
        for (band_idx, band_lock) in self.bands.iter().enumerate() {
            let key = Self::band_value(fingerprint, band_idx);
            let band = band_lock.read();
            if let Some(bucket) = band.get(&key) {
                for &id in bucket {
                    candidates.insert(id);
                }
            }
        }

        if candidates.is_empty() {
            return Ok(DedupStatus::New);
        }

        let rev = self.reverse.read();
        let mut matches: Vec<(DocId, u8)> = vec![];

        for &doc_id in &candidates {
            if let Some(entry) = rev.get(&doc_id) {
                #[allow(clippy::cast_possible_truncation)]
                let dist = (fingerprint ^ entry.fingerprint).count_ones() as u8;
                if dist <= self.config.hamming_threshold {
                    matches.push((doc_id, dist));
                }
            }
        }

        if matches.is_empty() {
            return Ok(DedupStatus::New);
        }

        matches.sort_by_key(|&(_, d)| d);
        let (best_doc, best_dist) = matches[0];

        if best_dist == 0 {
            Ok(DedupStatus::ExactDuplicate { existing: best_doc })
        } else {
            Ok(DedupStatus::NearDuplicate {
                best_match: best_doc,
                hamming_distance: best_dist,
            })
        }
    }
}
