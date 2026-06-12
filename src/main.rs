mod app;
mod memory;
mod types;
mod ui;

use app::{App, ChatMessage, Role, TaskResult};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use memory::MemoryStore;
use ratatui::{Terminal, backend::CrosstermBackend};
use rig_core::client::{CompletionClient, ProviderClient};
use rig_core::completion::{AssistantContent, Chat, Message, Prompt};
use rig_core::extractor::Extractor;
use rig_core::message::UserContent;
use rig_core::providers::openai::CompletionsClient;
use rig_core::schemars;
use std::io;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

const MAX_HISTORY: usize = 20;
const COMPRESS_KEEP: usize = 10;

const PREAMBLE: &str = "Kamu adalah AI personal assistant yang cerdas, ramah, dan memiliki memori jangka panjang. \
                        Jawab selalu dalam Bahasa Indonesia. \
                        Kamu akan diberikan konteks dari memori percakapan sebelumnya - gunakan itu untuk \
                        memberikan respons yang personal dan relevan.";

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
struct ExtractedFacts {
    /// Daftar fakta penting tentang user yang layak diingat jangka panjang
    facts: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Memuat AI Personal Assistant...");

    let client = CompletionsClient::from_env()?;

    let agent = Arc::new(
        client
            .agent("nvidia/nemotron-3-super-120b-a12b:free")
            .preamble(PREAMBLE)
            .build(),
    );
    let fallback_agent = Arc::new(
        client
            .agent("nvidia/nemotron-3-super-120b-a12b:free")
            .preamble(PREAMBLE)
            .build(),
    );
    let extractor = Arc::new(
        client
            .extractor::<ExtractedFacts>("nvidia/nemotron-3-super-120b-a12b:free")
            .preamble(
                "Ekstrak fakta penting tentang user dari percakapan: preferensi, \
                info pribadi, tujuan, dan rencana. Hanya fakta yang berguna untuk \
                jangka panjang. Tulis dalam Bahasa Indonesia sudut pandang ketiga. \
                Jika tidak ada fakta penting, kembalikan daftar kosong.",
            )
            .build(),
    );

    let memory = Arc::new(Mutex::new(MemoryStore::load(&client).await?));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut app = App::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<TaskResult>();

    'main: loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        while let Ok(result) = rx.try_recv() {
            match result {
                TaskResult::AiResponse {
                    response,
                    updated_history,
                    memory_count,
                } => {
                    if memory_count > 0 {
                        app.messages.push(ChatMessage {
                            role: Role::System,
                            content: format!("🧠 {} memori relevan digunakan", memory_count),
                        });
                    }
                    app.messages.push(ChatMessage {
                        role: Role::Ai,
                        content: response,
                    });
                    app.rig_history = updated_history;
                    app.is_loading = false;
                }
                TaskResult::SystemMessage(msg) => {
                    app.messages.push(ChatMessage {
                        role: Role::System,
                        content: msg,
                    });
                    app.is_loading = false;
                }
                TaskResult::Error(e) => {
                    app.messages.push(ChatMessage {
                        role: Role::System,
                        content: format!("⚠️ Error: {}", e),
                    });
                    app.is_loading = false;
                }
            }
        }

        if app.is_loading {
            app.tick();
        }

        tokio::time::sleep(std::time::Duration::from_millis(16)).await;

        if event::poll(std::time::Duration::ZERO)?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.messages.push(ChatMessage {
                        role: Role::System,
                        content: "💾 Menyimpan memori sebelum keluar...".into(),
                    });
                    terminal.draw(|f| ui::draw(f, &app))?;
                    let history_text = messages_to_text(&app.rig_history);
                    if !history_text.is_empty() {
                        save_memories_on_exit(&extractor, &memory, &history_text).await;
                    }
                    break 'main;
                }
                KeyCode::Enter if !app.is_loading && !app.input.is_empty() => {
                    let input = std::mem::take(&mut app.input);
                    app.messages.push(ChatMessage {
                        role: Role::User,
                        content: input.clone(),
                    });
                    app.is_loading = true;

                    if let Some(fact) = input.strip_prefix("remember:") {
                        let fact = fact.trim().to_string();
                        let tx = tx.clone();
                        let mem = Arc::clone(&memory);
                        tokio::spawn(async move {
                            let mut store = mem.lock().await;
                            let msg = match store.add(&fact, "manual").await {
                                Ok(true) => format!("✅ Tersimpan ke memori: \"{}\"", fact),
                                Ok(false) => "ℹ️  Sudah ada di memori".into(),
                                Err(e) => format!("⚠️ Gagal menyimpan: {}", e),
                            };
                            tx.send(TaskResult::SystemMessage(msg)).ok();
                        });
                    } else {
                        let tx = tx.clone();
                        let agent = Arc::clone(&agent);
                        let fallback = Arc::clone(&fallback_agent);
                        let mem = Arc::clone(&memory);
                        let extractor = Arc::clone(&extractor);
                        let mut history = app.rig_history.clone();

                        tokio::spawn(async move {
                            let memories = {
                                let store = mem.lock().await;
                                store.search(&input, 3).await.unwrap_or_default()
                            };
                            let memory_count = memories.len();
                            let prompt = if memories.is_empty() {
                                input.clone()
                            } else {
                                format!(
                                    "[Konteks dari memori tentang user]\n{}\n\n[Pertanyaan user]\n{}",
                                    memories.join("\n- "),
                                    input
                                )
                            };
                            let response = match agent.chat(&prompt, &mut history).await {
                                Ok(r) => r,
                                Err(e) => match fallback.chat(&prompt, &mut history).await {
                                    Ok(r) => r,
                                    Err(_) => {
                                        tx.send(TaskResult::Error(e.to_string())).ok();
                                        return;
                                    }
                                },
                            };

                            let compressed =
                                compress_history_if_needed(&mut history, &*agent, &extractor, &mem)
                                    .await;
                            if compressed {
                                tx.send(TaskResult::SystemMessage(
                                    "🗜️ Percakapan lama dirangkum dan disimpan ke memori".into(),
                                ))
                                .ok();
                            }

                            tx.send(TaskResult::AiResponse {
                                response,
                                updated_history: history,
                                memory_count,
                            })
                            .ok();
                        });
                    }
                }
                KeyCode::Backspace if !app.is_loading => {
                    app.input.pop();
                }
                KeyCode::Char(c)
                    if !app.is_loading
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    app.input.push(c);
                }
                _ => {}
            }
        }
    }

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    Ok(())
}

fn messages_to_text(messages: &[Message]) -> String {
    messages
        .iter()
        .filter_map(|msg| match msg {
            Message::User { content } => content.iter().find_map(|c| {
                if let UserContent::Text(t) = c {
                    Some(format!("User: {}", t.text))
                } else {
                    None
                }
            }),
            Message::Assistant { content, .. } => content.iter().find_map(|c| {
                if let AssistantContent::Text(t) = c {
                    Some(format!("AI: {}", t.text))
                } else {
                    None
                }
            }),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn compress_history_if_needed(
    history: &mut Vec<Message>,
    agent: &impl Prompt,
    extractor: &Extractor<<CompletionsClient as CompletionClient>::CompletionModel, ExtractedFacts>,
    memory: &Arc<Mutex<MemoryStore>>,
) -> bool {
    if history.len() <= MAX_HISTORY {
        return false;
    }

    let split = history.len() - COMPRESS_KEEP;
    let old_messages: Vec<Message> = history.drain(..split).collect();
    let text = messages_to_text(&old_messages);

    if let Ok(extracted) = extractor.extract(&text).await {
        let mut store = memory.lock().await;
        for fact in extracted.facts {
            let _ = store.add(&fact, "auto").await;
        }
    }

    let summary_prompt = format!(
        "Rangkum percakapan berikut menjadi 1 paragraf ringkas dalam Bahasa Indonesia:\n\n{}",
        text
    );
    if let Ok(summary) = agent.prompt(&summary_prompt).await {
        history.insert(
            0,
            Message::user(format!("[Ringkasan percakapan sebelumnya]\n{}", summary)),
        );
    }

    true
}

async fn save_memories_on_exit(
    extractor: &Extractor<<CompletionsClient as CompletionClient>::CompletionModel, ExtractedFacts>,
    memory: &Arc<Mutex<MemoryStore>>,
    conversation_text: &str,
) {
    if let Ok(extracted) = extractor.extract(conversation_text).await {
        let mut store = memory.lock().await;
        for fact in extracted.facts {
            let _ = store.add(&fact, "auto").await;
        }
    }
}
