//! The production embedder: a local sentence-transformer (BERT) run with candle.
//!
//! The model weights (`tokenizer.json`, `config.json`, `model.safetensors`) are fetched on demand
//! from the Hugging Face hub and cached under [`embedding_model_dir`] — i.e. under the tddy data
//! dir — so the first session fetches once and later sessions reuse the cache. The heavy ML stack
//! (candle + tokenizers + hf-hub) is pulled only with the non-default `local-model` feature; a build
//! without it cannot produce an embedder ([`production_embedder`] returns an error), so ordinary
//! `cargo test` stays model-free and offline.
//!
//! The cache location is resolved regardless of feature so callers (and tests) can reason about
//! where weights live without pulling the ML stack.
//!
//! Note: a future `tddy-action` may drive the weight fetch out-of-band (pre-warming the cache); the
//! direct hf-hub fetch here is the current, self-contained path.

use std::path::{Path, PathBuf};

use crate::embed::Embedder;

/// The sentence-transformer used for production embeddings: BERT, 384-dimensional.
#[cfg(feature = "local-model")]
const MODEL_REPO: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// The dimensionality of every vector [`LocalEmbedder`] produces.
#[cfg(feature = "local-model")]
const EMBEDDING_DIMENSIONS: usize = 384;

/// The on-disk cache directory for the embedding model's weights, under the tddy `data_dir`.
///
/// Always compiled (no feature gate) so the daemon and tests can locate/prepare the cache without
/// pulling the ML stack. Fetched weights are reused across sessions from this location.
pub fn embedding_model_dir(data_dir: &Path) -> PathBuf {
    data_dir
        .join("semantic-index")
        .join("models")
        .join("all-MiniLM-L6-v2")
}

/// Construct the production embedder, caching model weights under [`embedding_model_dir`].
///
/// With the `local-model` feature this returns the candle [`LocalEmbedder`], fetching and caching
/// weights on first use. Without the feature there is no embedder to produce, so this is an error
/// rather than a silent stub — a build that must index has to be built with the feature.
#[cfg(feature = "local-model")]
pub fn production_embedder(data_dir: &Path) -> anyhow::Result<Box<dyn Embedder>> {
    Ok(Box::new(LocalEmbedder::load(data_dir)?))
}

/// Without the `local-model` feature, no local model is compiled in, so no embedder can be produced.
#[cfg(not(feature = "local-model"))]
pub fn production_embedder(_data_dir: &Path) -> anyhow::Result<Box<dyn Embedder>> {
    anyhow::bail!("semantic index requires the 'local-model' build feature")
}

#[cfg(feature = "local-model")]
mod inference {
    use std::sync::Arc;

    use anyhow::Context;
    use async_trait::async_trait;
    use candle_core::{Device, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config, DTYPE};
    use hf_hub::api::sync::{Api, ApiBuilder};
    use hf_hub::{Repo, RepoType};
    use tokenizers::Tokenizer;

    use super::{embedding_model_dir, EMBEDDING_DIMENSIONS, MODEL_REPO};
    use crate::embed::Embedder;
    use std::path::Path;

    /// A local BERT sentence-transformer run with candle. Embeds text by mean-pooling the token
    /// embeddings under the attention mask and L2-normalizing (the standard sentence-transformers
    /// recipe), yielding 384-dimensional unit vectors.
    pub struct LocalEmbedder {
        inner: Arc<Model>,
    }

    /// The loaded model + tokenizer. Held behind an `Arc` so blocking inference can run on the
    /// runtime's blocking pool without borrowing `&self` across the await point.
    struct Model {
        model: BertModel,
        tokenizer: Tokenizer,
        device: Device,
    }

    impl LocalEmbedder {
        /// Load (fetching+caching on first use) the model weights under [`embedding_model_dir`].
        pub fn load(data_dir: &Path) -> anyhow::Result<Self> {
            let cache_dir = embedding_model_dir(data_dir);
            std::fs::create_dir_all(&cache_dir)
                .with_context(|| format!("create model cache dir {}", cache_dir.display()))?;

            let api: Api = ApiBuilder::new()
                .with_cache_dir(cache_dir)
                .build()
                .context("build hugging-face hub client")?;
            let repo = api.repo(Repo::new(MODEL_REPO.to_string(), RepoType::Model));

            let config_path = repo
                .get("config.json")
                .with_context(|| format!("fetch {MODEL_REPO}/config.json"))?;
            let tokenizer_path = repo
                .get("tokenizer.json")
                .with_context(|| format!("fetch {MODEL_REPO}/tokenizer.json"))?;
            let weights_path = repo
                .get("model.safetensors")
                .with_context(|| format!("fetch {MODEL_REPO}/model.safetensors"))?;

            let config: Config = serde_json::from_slice(
                &std::fs::read(&config_path).context("read model config.json")?,
            )
            .context("parse model config.json")?;
            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| anyhow::anyhow!("load tokenizer.json: {e}"))?;

            let device = Device::Cpu;
            // SAFETY: `from_mmaped_safetensors` memory-maps the weights file we just fetched into a
            // read-only view for the lifetime of the model; the file is not mutated concurrently.
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)
                    .context("map model safetensors")?
            };
            let model = BertModel::load(vb, &config).context("load BERT weights")?;

            Ok(Self {
                inner: Arc::new(Model {
                    model,
                    tokenizer,
                    device,
                }),
            })
        }
    }

    #[async_trait]
    impl Embedder for LocalEmbedder {
        async fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let inner = self.inner.clone();
            let texts = texts.to_vec();
            // Inference is CPU-bound; run it on the blocking pool so it never stalls the reactor.
            tokio::task::spawn_blocking(move || inner.embed_blocking(&texts))
                .await
                .context("embedding task panicked")?
        }

        fn dimensions(&self) -> usize {
            EMBEDDING_DIMENSIONS
        }
    }

    impl Model {
        /// Tokenize, run the encoder, mean-pool under the attention mask, then L2-normalize.
        fn embed_blocking(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            let encodings = self
                .tokenizer
                .encode_batch(texts.to_vec(), true)
                .map_err(|e| anyhow::anyhow!("tokenize inputs: {e}"))?;

            let batch = encodings.len();
            let seq_len = encodings
                .iter()
                .map(|e| e.get_ids().len())
                .max()
                .unwrap_or(0);
            anyhow::ensure!(seq_len > 0, "tokenizer produced no tokens for the inputs");

            // Right-pad every sequence to the batch's longest so the tensors are rectangular; the
            // attention mask zeroes out the padding for both the encoder and the pooling.
            let mut ids = Vec::with_capacity(batch * seq_len);
            let mut mask = Vec::with_capacity(batch * seq_len);
            for enc in &encodings {
                let enc_ids = enc.get_ids();
                let enc_mask = enc.get_attention_mask();
                for i in 0..seq_len {
                    ids.push(*enc_ids.get(i).unwrap_or(&0));
                    mask.push(*enc_mask.get(i).unwrap_or(&0) as f32);
                }
            }

            let input_ids = Tensor::from_vec(ids, (batch, seq_len), &self.device)
                .context("build input_ids tensor")?;
            let attention_mask = Tensor::from_vec(mask, (batch, seq_len), &self.device)
                .context("build attention_mask tensor")?;
            let token_type_ids = input_ids.zeros_like().context("build token_type_ids")?;

            let token_embeddings = self
                .model
                .forward(&input_ids, &token_type_ids, Some(&attention_mask))
                .context("run BERT encoder")?;

            // Mean-pool over the sequence dimension, weighting by the attention mask so padding
            // contributes nothing, then divide by the number of real tokens.
            let mask_3d = attention_mask
                .unsqueeze(2)
                .context("expand attention mask")?;
            let summed = token_embeddings
                .broadcast_mul(&mask_3d)
                .context("mask token embeddings")?
                .sum(1)
                .context("sum token embeddings")?;
            let counts = mask_3d.sum(1).context("count real tokens")?;
            let mean = summed.broadcast_div(&counts).context("mean-pool")?;

            // L2-normalize so cosine similarity is a plain dot product downstream.
            let norm = mean
                .sqr()
                .context("square pooled embeddings")?
                .sum_keepdim(1)
                .context("sum squares")?
                .sqrt()
                .context("l2 norm")?;
            let normalized = mean.broadcast_div(&norm).context("normalize embeddings")?;

            normalized
                .to_vec2::<f32>()
                .context("read embedding vectors")
        }
    }
}

#[cfg(feature = "local-model")]
pub use inference::LocalEmbedder;
