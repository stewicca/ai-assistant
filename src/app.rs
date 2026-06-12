use rig_core::completion::Message;

#[derive(Clone, PartialEq)]
pub enum Role {
    User,
    Ai,
    System,
}

#[derive(Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

pub enum TaskResult {
    AiResponse {
        response: String,
        updated_history: Vec<Message>,
        memory_count: usize,
    },
    SystemMessage(String),
    Error(String),
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub is_loading: bool,
    pub spinner_frame: usize,
    pub rig_history: Vec<Message>,
}

impl App {
    pub fn new() -> Self {
        App {
            messages: vec![ChatMessage {
                role: Role::System,
                content: "Halo! Ketik pesan lalu Enter. \"remember: <fakta>\" untuk simpan ke memori. Ctrl+C untuk keluar.".into(),
            }],
            input: String::new(),
            is_loading: false,
            spinner_frame: 0,
            rig_history: Vec::new(),
        }
    }

    pub fn tick(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER.len();
    }

    pub fn spinner(&self) -> &str {
        SPINNER[self.spinner_frame]
    }
}
