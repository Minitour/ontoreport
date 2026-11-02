//! User configuration in `~/.ontoreport/config.toml`, and the folder models are downloaded to.
//!
//! The folder is `~/.ontoreport` unless `ONTOREPORT_HOME` names another one. The configuration file is
//! written with the defaults, commented, the first time ontoreport runs, so the user can see and change
//! which embedding model the coherence and corpus-fit sections use.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The configuration file written on first run. Every value is the default.
pub const DEFAULT_CONFIG: &str = r#"# ontoreport configuration. Delete this file to get the defaults back.

[model]
# Sentence-embedding model for the coherence and corpus-fit sections: a Hugging Face repository with
# config.json, tokenizer.json and model.safetensors, of the BERT family (BERT, MiniLM, BGE, ...).
repo = "sentence-transformers/all-MiniLM-L6-v2"
# Commit of that repository. Pinned, so every machine downloads the same weights.
revision = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
# How token vectors become one vector per text: "mean" (sentence-transformers models) or "cls" (BGE).
pooling = "mean"
# Longest text, in tokens; longer texts are cut.
max_tokens = 128
# Texts per forward pass. Batches run in parallel, one per CPU core; small batches waste less work on
# padding, and 8 was fastest on ENVO.
batch_size = 8
# Folder models are downloaded to. Default: the "models" folder next to this file.
# dir = "D:/models"

[text]
# What is embedded for each class: "label", or "label+definition" (the label, a colon, then the
# definition when there is one). Label plus definition agreed best with the domain experts' judgments
# in our model comparison; "label" is what the 2023 corpus-based evaluation used.
form = "label+definition"
"#;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct File {
    model: ModelFile,
    text: TextFile,
}

// The defaults below must match DEFAULT_CONFIG; a test checks that they do.
const DEFAULT_REPO: &str = "sentence-transformers/all-MiniLM-L6-v2";
const DEFAULT_REVISION: &str = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ModelFile {
    repo: String,
    revision: String,
    pooling: Pooling,
    max_tokens: usize,
    batch_size: usize,
    dir: Option<PathBuf>,
}

impl Default for ModelFile {
    fn default() -> Self {
        ModelFile {
            repo: DEFAULT_REPO.into(),
            revision: DEFAULT_REVISION.into(),
            pooling: Pooling::Mean,
            max_tokens: 128,
            batch_size: 8,
            dir: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TextFile {
    form: TextForm,
}

impl Default for TextFile {
    fn default() -> Self {
        TextFile {
            form: TextForm::LabelDefinition,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pooling {
    Mean,
    Cls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TextForm {
    #[serde(rename = "label")]
    Label,
    #[serde(rename = "label+definition")]
    LabelDefinition,
}

impl TextForm {
    pub fn name(self) -> &'static str {
        match self {
            TextForm::Label => "label",
            TextForm::LabelDefinition => "label+definition",
        }
    }
}

/// The embedding model to use and where its files live.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub repo: String,
    pub revision: String,
    pub pooling: Pooling,
    pub max_tokens: usize,
    pub batch_size: usize,
    /// Folder holding this model's files: `<models dir>/<repo with / as __>/<revision>`.
    pub dir: PathBuf,
}

impl ModelConfig {
    /// Short name for the report: the repository and the first 7 characters of the revision.
    pub fn name(&self) -> String {
        format!(
            "{}@{}",
            self.repo,
            &self.revision[..self.revision.len().min(7)]
        )
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// `~/.ontoreport` or `ONTOREPORT_HOME`.
    pub home: PathBuf,
    /// The configuration file that was read.
    pub path: PathBuf,
    pub model: ModelConfig,
    pub text: TextForm,
}

impl Config {
    /// Embedding cache file for this model and text settings.
    pub fn cache_file(&self) -> PathBuf {
        let m = &self.model;
        let slug = format!(
            "{}-{}-{}-{}.vec",
            m.repo.replace(['/', '\\', ':'], "__"),
            &m.revision[..m.revision.len().min(12)],
            match m.pooling {
                Pooling::Mean => "mean",
                Pooling::Cls => "cls",
            },
            m.max_tokens
        );
        self.home.join("cache").join(slug)
    }
}

/// `ONTOREPORT_HOME`, else `.ontoreport` in the user's home folder.
pub fn home() -> Result<PathBuf, String> {
    if let Some(h) = std::env::var_os("ONTOREPORT_HOME").filter(|h| !h.is_empty()) {
        return Ok(PathBuf::from(h));
    }
    let user = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))
        .ok_or("no home folder: set HOME, USERPROFILE or ONTOREPORT_HOME")?;
    Ok(PathBuf::from(user).join(".ontoreport"))
}

/// Read the configuration. With no `path`, read `<home>/config.toml`, writing it with the defaults
/// first if it does not exist yet.
pub fn load(path: Option<&Path>) -> Result<Config, String> {
    let home = home()?;
    let path = match path {
        Some(p) => p.to_path_buf(),
        None => {
            let p = home.join("config.toml");
            if !p.exists() {
                fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
                fs::write(&p, DEFAULT_CONFIG).map_err(|e| format!("{}: {e}", p.display()))?;
            }
            p
        }
    };
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: File = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let m = file.model;
    if m.repo.trim().is_empty() || m.revision.trim().is_empty() {
        return Err(format!(
            "{}: model.repo and model.revision must be set",
            path.display()
        ));
    }
    let models = match m.dir {
        Some(d) if d.is_absolute() => d,
        Some(d) => path.parent().unwrap_or(Path::new(".")).join(d),
        None => home.join("models"),
    };
    let dir = models
        .join(m.repo.replace(['/', '\\', ':'], "__"))
        .join(&m.revision);
    Ok(Config {
        home,
        path,
        model: ModelConfig {
            repo: m.repo,
            revision: m.revision,
            pooling: m.pooling,
            max_tokens: m.max_tokens.max(8),
            batch_size: m.batch_size.max(1),
            dir,
        },
        text: file.text.form,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_config_matches_the_built_in_defaults() {
        let written: File = toml::from_str(DEFAULT_CONFIG).unwrap();
        let built_in = File::default();
        assert_eq!(format!("{written:?}"), format!("{built_in:?}"));
        assert_eq!(written.model.repo, DEFAULT_REPO);
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let f: File = toml::from_str("[text]\nform = \"label\"\n").unwrap();
        assert_eq!(f.text.form, TextForm::Label);
        assert_eq!(f.model.max_tokens, 128);
    }

    #[test]
    fn unknown_keys_are_errors() {
        assert!(toml::from_str::<File>("[model]\nrepo = \"x\"\nrevison = \"y\"\n").is_err());
    }
}
