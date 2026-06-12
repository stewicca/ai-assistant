use crate::types::MemoryEntry;
use rig_core::client::EmbeddingsClient;
use rig_core::embeddings::EmbeddingModel;
use rig_core::providers::openai::CompletionsClient;
use std::path::PathBuf;

const MEMORY_FILE: &str = "memory.json";

pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
    embeddings: Vec<Vec<f64>>,
    model: <CompletionsClient as EmbeddingsClient>::EmbeddingModel,
}

impl MemoryStore {
    pub async fn load(client: &CompletionsClient) -> anyhow::Result<Self> {
        let model = client.embedding_model("nvidia/llama-nemotron-embed-vl-1b-v2:free");
        let entries = Self::load_from_file()?;

        let mut store = Self {
            entries: vec![],
            embeddings: vec![],
            model,
        };

        let mut needs_save = false;

        for entry in entries {
            let embedding = match &entry.embedding {
                Some(emb) => emb.clone(),
                None => {
                    needs_save = true;

                    store.embed(&entry.content).await?
                }
            };

            store.embeddings.push(embedding.clone());

            let mut entry = entry;

            entry.embedding = Some(embedding);

            store.entries.push(entry);
        }

        if needs_save {
            store.save_to_file()?;

            println!("💾 Embedding baru disimpan ke disk");
        }

        println!("📚 Loaded {} memories from disk", store.entries.len());

        Ok(store)
    }

    pub async fn add(&mut self, content: &str, source: &str) -> anyhow::Result<bool> {
        let mut entry = MemoryEntry::new(content, source);
        let embedding = self.embed(content).await?;

        let is_duplicate = self
            .embeddings
            .iter()
            .any(|emb| cosine_similarity(&embedding, emb) > 0.75);

        if is_duplicate {
            return Ok(false);
        }

        entry.embedding = Some(embedding.clone());

        self.entries.push(entry);
        self.embeddings.push(embedding);
        self.save_to_file()?;

        Ok(true)
    }

    pub async fn search(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<String>> {
        if self.entries.is_empty() {
            return Ok(vec![]);
        }

        let query_embedding = self.embed(query).await?;

        let mut scores: Vec<(f64, usize)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(i, emb)| (cosine_similarity(&query_embedding, emb), i))
            .collect();

        scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        let results = scores
            .iter()
            .take(top_k)
            .filter(|(score, _)| *score > 0.15)
            .map(|(_, i)| self.entries[*i].content.clone())
            .collect();

        Ok(results)
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f64>> {
        let embeddings = self
            .model
            .embed_texts(vec![text.to_string()])
            .await
            .map_err(|e| anyhow::anyhow!("Embedding error: {}", e))?;

        let vec = embeddings
            .first()
            .ok_or_else(|| anyhow::anyhow!("Empty embedding response"))?
            .vec
            .clone();

        Ok(vec)
    }

    fn load_from_file() -> anyhow::Result<Vec<MemoryEntry>> {
        let path = PathBuf::from(MEMORY_FILE);

        if !path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(path)?;
        let entries = serde_json::from_str(&content)?;

        Ok(entries)
    }

    fn save_to_file(&self) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(&self.entries)?;

        std::fs::write(MEMORY_FILE, content)?;

        Ok(())
    }
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
