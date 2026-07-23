//! The [`Embedder`] boundary — turns text into fixed-dimension embedding vectors.
//!
//! The production implementation ([`crate::local_model::LocalEmbedder`]) wraps a local candle BERT
//! model behind the non-default `local-model` feature; tests inject a deterministic fake.

use async_trait::async_trait;

/// Produces embedding vectors for text. The production implementation wraps a local bundled model
/// (candle/ONNX, behind the non-default `local-model` feature); tests inject a deterministic fake so
/// indexing and ranking are exact and offline.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed each input text into a vector of length [`Embedder::dimensions`]. The returned vector
    /// count equals `texts.len()`, in order.
    async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;

    /// The dimensionality of every vector this embedder produces.
    fn dimensions(&self) -> usize;
}

/// Boxing an [`Embedder`] keeps it an [`Embedder`], so a `Box<dyn Embedder>` (e.g. the one
/// [`crate::production_embedder`] returns) satisfies the generic `E: Embedder` bound on
/// [`crate::SemanticIndexTask`] and the daemon's blocking-index step.
#[async_trait]
impl Embedder for Box<dyn Embedder> {
    async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        (**self).embed(texts).await
    }

    fn dimensions(&self) -> usize {
        (**self).dimensions()
    }
}
