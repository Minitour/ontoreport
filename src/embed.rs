//! Sentence embeddings for the coherence and corpus-fit sections.
//!
//! The model named in the configuration is downloaded once into the models folder (with `curl`, which
//! ships with Windows 10 and later, macOS and Linux) and run on the CPU with candle. Vectors are kept in a
//! cache file per model, so a second report on the same ontology embeds nothing and gives the same
//! vectors, and therefore the same report, byte for byte.

use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

use crate::config::{Config, ModelConfig, Pooling};

/// Files a model folder needs.
pub const MODEL_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

/// Unit-length vectors, one per distinct text.
#[derive(Debug, Default, Clone)]
pub struct Vectors {
    pub model: String,
    pub by_text: HashMap<String, Vec<f32>>,
    /// Texts taken from the cache and texts embedded in this run.
    pub cached: usize,
    pub computed: usize,
}

impl Vectors {
    pub fn get(&self, text: &str) -> Option<&[f32]> {
        self.by_text.get(text).map(|v| v.as_slice())
    }
}

/// Anything that turns texts into unit-length vectors. Tests use a stand-in; reports use [`Bert`].
pub trait Embedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String>;
}

/// Download the model's files into its folder unless they are there already.
pub fn fetch(m: &ModelConfig) -> Result<(), String> {
    let missing: Vec<&str> = MODEL_FILES
        .iter()
        .copied()
        .filter(|f| !m.dir.join(f).is_file())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(&m.dir).map_err(|e| format!("{}: {e}", m.dir.display()))?;
    for f in missing {
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{f}",
            m.repo, m.revision
        );
        let dest = m.dir.join(f);
        let part = m.dir.join(format!("{f}.part"));
        eprintln!("ontoreport: downloading {url}");
        let mut cmd = Command::new("curl");
        cmd.args(["-fL", "--retry", "3", "--silent", "--show-error", "-o"])
            .arg(&part)
            .arg(&url);
        if let Ok(token) = std::env::var("HF_TOKEN") {
            cmd.arg("-H").arg(format!("Authorization: Bearer {token}"));
        }
        let status = cmd.status().map_err(|e| {
            format!(
                "could not run curl to download the model ({e}); download {url} into {} by hand",
                m.dir.display()
            )
        })?;
        if !status.success() {
            let _ = fs::remove_file(&part);
            return Err(format!(
                "downloading {url} failed ({status}); check model.repo and model.revision in the configuration"
            ));
        }
        fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(())
}

/// A BERT-family sentence-embedding model, run on the CPU.
pub struct Bert {
    model: BertModel,
    tokenizer: Tokenizer,
    pooling: Pooling,
    batch: usize,
    device: Device,
}

impl Bert {
    /// Load the configured model, downloading it first if needed.
    pub fn load(m: &ModelConfig) -> Result<Bert, String> {
        fetch(m)?;
        let err = |what: &str, e: &dyn std::fmt::Display| format!("{} {what}: {e}", m.name());
        let device = Device::Cpu;
        let cfg_text =
            fs::read_to_string(m.dir.join("config.json")).map_err(|e| err("config.json", &e))?;
        let cfg: BertConfig =
            serde_json::from_str(&cfg_text).map_err(|e| err("config.json", &e))?;
        // SAFETY: the weights file is only read, and ontoreport does not write it while it is mapped.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[m.dir.join("model.safetensors")],
                DType::F32,
                &device,
            )
        }
        .map_err(|e| err("model.safetensors", &e))?;
        let model = BertModel::load(vb, &cfg).map_err(|e| err("weights", &e))?;
        let mut tokenizer = Tokenizer::from_file(m.dir.join("tokenizer.json"))
            .map_err(|e| err("tokenizer.json", &e))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: m.max_tokens,
                ..Default::default()
            }))
            .map_err(|e| err("tokenizer.json", &e))?;
        Ok(Bert {
            model,
            tokenizer,
            pooling: m.pooling,
            batch: m.batch_size,
            device,
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> candle_core::Result<Vec<Vec<f32>>> {
        let enc = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(candle_core::Error::msg)?;
        let ids = enc
            .iter()
            .map(|e| Tensor::new(e.get_ids(), &self.device))
            .collect::<candle_core::Result<Vec<_>>>()?;
        let mask = enc
            .iter()
            .map(|e| Tensor::new(e.get_attention_mask(), &self.device))
            .collect::<candle_core::Result<Vec<_>>>()?;
        let ids = Tensor::stack(&ids, 0)?;
        let mask = Tensor::stack(&mask, 0)?;
        let types = ids.zeros_like()?;
        let hidden = self.model.forward(&ids, &types, Some(&mask))?;
        let pooled = match self.pooling {
            Pooling::Cls => hidden.i((.., 0))?,
            Pooling::Mean => {
                let m = mask.to_dtype(DType::F32)?.unsqueeze(2)?;
                let sum = hidden.broadcast_mul(&m)?.sum(1)?;
                sum.broadcast_div(&m.sum(1)?)?
            }
        };
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        pooled.broadcast_div(&norm)?.to_vec2::<f32>()
    }
}

impl Embedder for Bert {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        // Batches of similar length waste less work on padding.
        let mut order: Vec<usize> = (0..texts.len()).collect();
        order.sort_by_key(|&i| (texts[i].len(), i));
        let chunks: Vec<&[usize]> = order.chunks(self.batch).collect();
        // One batch per core, each with single-threaded matrix products: on ENVO this is about six times
        // faster than one batch at a time with candle's multi-threaded products. Candle reads the thread
        // count from RAYON_NUM_THREADS; a value the user set is kept.
        if std::env::var_os("RAYON_NUM_THREADS").is_none() {
            std::env::set_var("RAYON_NUM_THREADS", "1");
        }
        let threads: usize = std::env::var("ONTOREPORT_EMBED_THREADS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
            .clamp(1, chunks.len().max(1));
        type Part = Result<Vec<(usize, Vec<f32>)>, String>;
        let results: Vec<Part> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let chunks = &chunks;
                    scope.spawn(move || {
                        let mut got = Vec::new();
                        for chunk in chunks.iter().skip(t).step_by(threads) {
                            let batch: Vec<&str> = chunk.iter().map(|&i| texts[i]).collect();
                            let vecs = self.embed_batch(&batch).map_err(|e| e.to_string())?;
                            got.extend(chunk.iter().copied().zip(vecs));
                        }
                        Ok(got)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err("embedding thread failed".into()))
                })
                .collect()
        });
        let mut out = vec![Vec::new(); texts.len()];
        for r in results {
            for (i, v) in r? {
                out[i] = v;
            }
        }
        Ok(out)
    }
}

/// Vectors for `texts`: from the cache where possible, the rest from the embedder, which is only
/// created when some text is not cached. New vectors are added to the cache.
pub fn vectors<F, E>(cfg: &Config, texts: &[String], make: F) -> Result<Vectors, String>
where
    F: FnOnce() -> Result<E, String>,
    E: Embedder,
{
    let path = cfg.cache_file();
    let mut cache = read_cache(&path);
    let mut wanted: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|t| !cache.contains_key(&key(t)))
        .collect();
    let mut computed = 0;
    if !missing.is_empty() {
        let t0 = Instant::now();
        let embedder = make()?;
        let vecs = embedder.embed(&missing)?;
        computed = missing.len();
        if std::env::var_os("ONTOREPORT_TIMING").is_some() {
            eprintln!(
                "  embedded {} texts in {:.1} s",
                computed,
                t0.elapsed().as_secs_f64()
            );
        }
        let fresh: Vec<(u64, Vec<f32>)> = missing.iter().map(|t| key(t)).zip(vecs).collect();
        if let Err(e) = append_cache(&path, &fresh) {
            eprintln!("ontoreport: could not write the embedding cache: {e}");
        }
        cache.extend(fresh);
    }
    let by_text = wanted
        .iter()
        .filter_map(|t| cache.get(&key(t)).map(|v| (t.to_string(), v.clone())))
        .collect();
    Ok(Vectors {
        model: cfg.model.name(),
        by_text,
        cached: wanted.len() - computed,
        computed,
    })
}

/// FNV-1a, 64 bits: a stable key for a text across runs and platforms.
fn key(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

const MAGIC: &[u8; 4] = b"ORV1";

/// Cache file: "ORV1", the dimension (u32), then records of key (u64) and vector (f32 each), all
/// little-endian. A file that is unreadable or cut short gives what it holds up to that point.
fn read_cache(path: &Path) -> HashMap<u64, Vec<f32>> {
    let mut map = HashMap::new();
    let Ok(mut f) = fs::File::open(path) else {
        return map;
    };
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() || buf.len() < 8 || &buf[..4] != MAGIC {
        return map;
    }
    let dim = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    let rec = 8 + 4 * dim;
    for r in buf[8..].chunks_exact(rec) {
        let k = u64::from_le_bytes(r[..8].try_into().unwrap());
        let v = r[8..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        map.insert(k, v);
    }
    map
}

fn append_cache(path: &Path, fresh: &[(u64, Vec<f32>)]) -> std::io::Result<()> {
    let Some(dim) = fresh.first().map(|(_, v)| v.len()) else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // A cache written for another dimension (a different model under the same name) is replaced.
    let header_ok = fs::read(path)
        .ok()
        .filter(|b| b.len() >= 8 && &b[..4] == MAGIC)
        .map(|b| u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize == dim)
        .unwrap_or(false);
    let file = if header_ok {
        fs::OpenOptions::new().append(true).open(path)?
    } else {
        let mut f = fs::File::create(path)?;
        f.write_all(MAGIC)?;
        f.write_all(&(dim as u32).to_le_bytes())?;
        f
    };
    let mut w = BufWriter::new(file);
    for (k, v) in fresh {
        w.write_all(&k.to_le_bytes())?;
        for x in v {
            w.write_all(&x.to_le_bytes())?;
        }
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.vec");
        append_cache(&p, &[(1, vec![0.5, -1.0]), (2, vec![1.0, 0.0])]).unwrap();
        append_cache(&p, &[(3, vec![0.0, 1.0])]).unwrap();
        let c = read_cache(&p);
        assert_eq!(c.len(), 3);
        assert_eq!(c[&1], vec![0.5, -1.0]);
        // A different dimension replaces the file.
        append_cache(&p, &[(4, vec![1.0, 0.0, 0.0])]).unwrap();
        assert_eq!(read_cache(&p).len(), 1);
    }

    #[test]
    fn keys_are_stable() {
        assert_eq!(key(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(key("a"), 0xaf63_dc4c_8601_ec8c);
    }
}
