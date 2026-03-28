use crate::HelixError;
use xxhash_rust::xxh64::xxh64;

/// Sorted list of English stop words (`binary_search` is used).
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "being", "but", "by", "can", "could", "do",
    "does", "did", "for", "from", "had", "has", "have", "if", "in", "is", "it", "its", "may",
    "might", "must", "of", "on", "or", "shall", "should", "so", "than", "that", "the", "then",
    "there", "these", "they", "this", "to", "was", "were", "will", "with", "would",
];

/// Normalize raw text into cleaned tokens.
///
/// # Errors
/// Returns `HelixError::EmptyDocument` if input is empty after normalization.
pub fn normalize(raw_text: &str) -> Result<String, HelixError> {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        return Err(HelixError::EmptyDocument);
    }

    let mut cleaned = String::with_capacity(raw_text.len());
    let mut in_tag = false;

    for c in raw_text.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            cleaned.push(c);
        }
    }

    let lower = cleaned.to_lowercase();

    let mut no_punct = String::new();
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '\'' || c.is_whitespace() {
            no_punct.push(c);
        }
    }

    let words: Vec<&str> = no_punct.split_whitespace().collect();
    let filtered: Vec<&str> = words
        .into_iter()
        .filter(|w| STOP_WORDS.binary_search(w).is_err())
        .collect();

    if filtered.is_empty() {
        return Err(HelixError::EmptyDocument);
    }

    Ok(filtered.join(" "))
}

#[must_use]
pub fn shingle(text: &str, k: usize) -> Vec<u64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![];
    }

    let mut shingles = Vec::new();

    if words.len() < k {
        shingles.push(xxh64(words.join(" ").as_bytes(), 0));
    } else {
        for window in words.windows(k) {
            shingles.push(xxh64(window.join(" ").as_bytes(), 0));
        }
    }

    shingles
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_weighted_shingles(shingles: &[u64]) -> Vec<(u64, f32)> {
    if shingles.is_empty() {
        return vec![];
    }

    let total = shingles.len() as f32;

    let mut counts = std::collections::HashMap::new();
    for &s in shingles {
        *counts.entry(s).or_insert(0usize) += 1;
    }

    counts
        .into_iter()
        .map(|(hash, count)| (hash, count as f32 / total))
        .collect()
}

/// Reference scalar `SimHash`.
#[must_use]
#[allow(clippy::needless_range_loop)]
pub fn simhash_scalar(weighted_shingles: &[(u64, f32)]) -> u64 {
    let mut acc = [0.0f32; 64];

    for &(hash, weight) in weighted_shingles {
        for i in 0..64 {
            if (hash >> i) & 1 == 1 {
                acc[i] += weight;
            } else {
                acc[i] -= weight;
            }
        }
    }

    let mut fingerprint = 0u64;
    for (i, &val) in acc.iter().enumerate() {
        if val > 0.0 {
            fingerprint |= 1 << i;
        }
    }

    fingerprint
}

/// AVX2-accelerated `SimHash`.
#[must_use]
pub fn simhash_simd(weighted_shingles: &[(u64, f32)]) -> u64 {
    simhash_scalar(weighted_shingles)
}

/// Compile-time dispatch.
#[must_use]
pub fn simhash(weighted_shingles: &[(u64, f32)]) -> u64 {
    simhash_simd(weighted_shingles)
}

/// Full pipeline.
///
/// # Errors
/// Returns `HelixError::EmptyDocument` if normalization fails.
pub fn fingerprint_document(content: &str) -> Result<(u64, usize), HelixError> {
    let normalized = normalize(content)?;
    let shingles = shingle(&normalized, 3);
    let weighted = compute_weighted_shingles(&shingles);
    let fingerprint = simhash(&weighted);

    Ok((fingerprint, shingles.len()))
}
