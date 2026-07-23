//! Shared test support: deterministic offline embedders and a chunk builder.
#![allow(dead_code)] // each test binary uses a subset of these helpers.

use async_trait::async_trait;
use tddy_semantic_index::{Embedder, IndexedChunk};

/// The dimensionality of the fake embedders below.
pub const FAKE_DIMS: usize = 32;

/// A deterministic, offline embedder: hashes each word into a bag-of-words vector, then
/// L2-normalizes. Texts that share words land near each other in cosine space, so ranking is exact
/// and reproducible with no model download or network.
pub struct HashingEmbedder;

#[async_trait]
impl Embedder for HashingEmbedder {
    async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| embed_one(t)).collect())
    }

    fn dimensions(&self) -> usize {
        FAKE_DIMS
    }
}

/// An embedder that always fails — drives the index task's failure/abort path.
pub struct FailingEmbedder;

#[async_trait]
impl Embedder for FailingEmbedder {
    async fn embed(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        anyhow::bail!("embedding backend unavailable")
    }

    fn dimensions(&self) -> usize {
        FAKE_DIMS
    }
}

/// Embed one text into a normalized bag-of-words vector (FNV-hashed tokens).
fn embed_one(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; FAKE_DIMS];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let mut h: u32 = 2166136261;
        for b in word.to_ascii_lowercase().bytes() {
            h = h.wrapping_mul(16777619) ^ (b as u32);
        }
        v[(h as usize) % FAKE_DIMS] += 1.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Build an [`IndexedChunk`] by embedding `text` with the given embedder.
pub async fn an_indexed_chunk(
    embedder: &HashingEmbedder,
    source_path: &str,
    text: &str,
) -> IndexedChunk {
    let vector = embedder
        .embed(&[text.to_string()])
        .await
        .expect("fake embedder is infallible")
        .into_iter()
        .next()
        .expect("one vector per text");
    IndexedChunk {
        source_path: source_path.into(),
        text: text.into(),
        vector,
    }
}
