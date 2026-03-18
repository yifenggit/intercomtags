// 英文版本界面（备用）
use crate::config::Config;
use crate::file_parser::{self, EmailWithTag};
use crate::intercom::{IntercomClient, TagResult};
use eframe::egui;
use egui::{Color32, FontId, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct IntercomTagsApp {
    config: Config,
    config_changed: bool,
    input_mode: InputMode,
    selected_file: Option<PathBuf>,
    file_emails: Vec<EmailWithTag>,
    manual_emails: String,
    manual_tag: String,
    is_running: bool,
    progress: f32,
    status_message: String,
    results: Vec<TagResult>,
    success_count: usize,
    failed_count: usize,
    log_messages: Vec<LogMessage>,
    rt: tokio::runtime::Runtime,
    progress_rx: Option<mpsc::Receiver<ProgressMessage>>,
}

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    File,
    Manual,
}

#[derive(Debug, Clone)]
enum ProgressMessage {
    Log(LogMessage),
    Result(TagResult),
    Progress(f32),
    Status(String),
    Finished { success: usize, failed: usize },
}

#[derive(Debug, Clone)]
struct LogMessage {
    level: LogLevel,
    message: String,
    timestamp: chrono::DateTime<chrono::Local>,
}

#[derive(Debug, Clone, PartialEq)]
enum LogLevel {
    Info,
    Warn,
    Error,
    Success,
}

impl IntercomTagsApp {
    pub fn new() -> Self {
        let config = Config::load();
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        Self {
            config,
            config_changed: false,
            input_mode: InputMode::File,
            selected_file: None,
            file_emails: Vec::new(),
            manual_emails: String::new(),
            manual_tag: String::new(),
            is_running: false,
            progress: 0.0,
            status_message: "Ready".to_string(),
            results: Vec::new(),
            success_count: 0,
            failed_count: 0,
            log_messages: Vec::new(),
            rt,
            progress_rx: None,
        }
    }

    fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.log_messages.push(LogMessage {
            level,
            message: message.into(),
            timestamp: chrono::Local::now(),
        });

        if self.log_messages.len() > 1000 {
            self.log_messages.remove(0);
        }
    }

    fn select_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV files", &["csv"])
            .add_filter("Excel files", &["xlsx", "xls"])
            .pick_file()
        {
            match file_parser::parse_file(&path) {
                Ok(emails) => {
                    self.selected_file = Some(path.clone());
                    self.file_emails = emails.clone();
                    self.log(
                        LogLevel::Success,
                        format!("Loaded file: {} ({} records)", 
                            path.display(), emails.len()),
                    );
                    self.status_message = format!("Loaded {} records", emails.len());
                }
                Err(e) => {
                    self.log(LogLevel::Error, format!("Failed to parse file: {}", e));
                    self.status_message = "File parse failed".to_string();
                }
            }
        }
    }

    fn parse_manual_emails(&self) -> Vec<EmailWithTag> {
        self.manual_emails
            .lines()
            .filter_map(|line| {
                let email = line.trim();
                if email.contains('@') && !email.is_empty() {
                    Some(EmailWithTag {
                        email: email.to_string(),
                        tag: if self.manual_tag.is_empty() {
                            None
                        } else {
                            Some(self.manual_tag.clone())
                        },
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn can_start(&self) -> bool {
        !self.is_running
            && !self.config.token.is_empty()
            && match self.input_mode {
                InputMode::File => !self.file_emails.is_empty(),
                InputMode::Manual => {
                    !self.manual_emails.trim().is_empty() && !self.manual_tag.is_empty()
                }
            }
    }

    fn start_execution(&mut self) {
        if !self.can_start() {
            return;
        }

        if self.config_changed {
            if let Err(e) = self.config.save() {
                self.log(LogLevel::Error, format!("Failed to save config: {}", e));
            } else {
                self.config_changed = false;
            }
        }

        let emails: Vec<EmailWithTag> = match self.input_mode {
            InputMode::File => self.file_emails.clone(),
            InputMode::Manual => self.parse_manual_emails(),
        };

        if emails.is_empty() {
            self.log(LogLevel::Error, "No valid email addresses");
            return;
        }

        self.is_running = true;
        self.progress = 0.0;
        self.results.clear();
        self.success_count = 0;
        self.failed_count = 0;
        self.status_message = "Starting...".to_string();

        let total_emails = emails.len();
        let manual_tag = self.manual_tag.clone();
        let input_mode = self.input_mode.clone();

        let mut tag_groups: HashMap<String, Vec<String>> = HashMap::new();
        for item in emails {
            let tag = item.tag.clone().unwrap_or_else(|| {
                if input_mode == InputMode::Manual {
                    manual_tag.clone()
                } else {
                    "default".to_string()
                }
            });
            tag_groups.entry(tag).or_default().push(item.email);
        }

        let token = self.config.token.clone();
        let retries = self.config.retries;

        let (tx, rx) = mpsc::channel(100);
        self.progress_rx = Some(rx);

        self.log(
            LogLevel::Info,
            format!(
                "Processing {} emails, {} tags",
                total_emails,
                tag_groups.len()
            ),
        );

        self.rt.spawn(async move {
            process_tags(tag_groups, token, retries, tx).await;
        });
    }

    fn update_progress(&mut self) {
        let mut messages = Vec::new();
        
        if let Some(rx) = &mut self.progress_rx {
            loop {
                match rx.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.progress_rx = None;
                        break;
                    }
                }
            }
        }

        for msg in messages {
            match msg {
                ProgressMessage::Log(log) => {
                    self.log_messages.push(log);
                }
                ProgressMessage::Result(result) => {
                    if result.success {
                        self.success_count += 1;
                    } else {
                        self.failed_count += 1;
                    }
                    self.results.push(result);
                }
                ProgressMessage::Progress(p) => {
                    self.progress = p;
                }
                ProgressMessage::Status(s) => {
                    self.status_message = s;
                }
                ProgressMessage::Finished { success, failed } => {
                    self.is_running = false;
                    self.success_count = success;
                    self.failed_count = failed;
                    self.progress = 100.0;
                    self.status_message = format!(
                        "Done! Success: {}, Failed: {}",
                        success, failed
                    );
                    self.log_messages.push(LogMessage {
                        level: LogLevel::Success,
                        message: format!("Completed! Success: {}, Failed: {}", success, failed),
                        timestamp: chrono::Local::now(),
                    });
                }
            }
        }
    }
}

async fn process_tags(
    tag_groups: HashMap<String, Vec<String>>,
    token: String,
    retries: u32,
    tx: mpsc::Sender<ProgressMessage>,
) {
    let client = IntercomClient::new(token, retries);
    let total_emails: usize = tag_groups.values().map(|v| v.len()).sum();
    let mut processed = 0usize;

    for (tag_name, emails) in tag_groups {
        let _ = tx
            .send(ProgressMessage::Log(LogMessage {
                level: LogLevel::Info,
                message: format!("Processing tag: {} ({} emails)", tag_name, emails.len()),
                timestamp: chrono::Local::now(),
            }))
            .await;

        match client.get_or_create_tag(&tag_name).await {
            Ok(tag) => {
                let _ = tx
                    .send(ProgressMessage::Log(LogMessage {
                        level: LogLevel::Success,
                        message: format!("Tag ready: {} (ID: {})", tag.name, tag.id),
                        timestamp: chrono::Local::now(),
                    }))
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(ProgressMessage::Log(LogMessage {
                        level: LogLevel::Error,
                        message: format!("Failed to get/create tag: {}", e),
                        timestamp: chrono::Local::now(),
                    }))
                    .await;
                continue;
            }
        }

        let batch_size = 50;
        for chunk in emails.chunks(batch_size) {
            let mut contact_ids = Vec::new();
            let mut failed_emails = Vec::new();

            let mut tasks = Vec::new();
            for email in chunk {
                let email = email.clone();
                tasks.push(tokio::spawn(async move {
                    (email.clone(), client.search_contact(&email).await)
                }));
            }

            for task in tasks {
                if let Ok((email, result)) = task.await {
                    match result {
                        Ok(Some(contact)) => contact_ids.push(contact.id),
                        Ok(None) => {
                            failed_emails.push(email);
                        }
                        Err(e) => {
                            let _ = tx
                                .send(ProgressMessage::Result(TagResult {
                                    email,
                                    success: false,
                                    message: format!("Search failed: {}", e),
                                }))
                                .await;
                        }
                    }
                }
            }

            for email in failed_emails {
                let _ = tx
                    .send(ProgressMessage::Result(TagResult {
                        email,
                        success: false,
                        message: "Contact not found".to_string(),
                    }))
                    .await;
                processed += 1;
            }

            if !contact_ids.is_empty() {
                match client.tag_contacts(&tag_name, contact_ids.clone()).await {
                    Ok(true) => {
                        let _ = tx
                            .send(ProgressMessage::Log(LogMessage {
                                level: LogLevel::Success,
                                message: format!("Tagged {} contacts", contact_ids.len()),
                                timestamp: chrono::Local::now(),
                            }))
                            .await;
                    }
                    Ok(false) => {
                        let _ = tx
                            .send(ProgressMessage::Log(LogMessage {
                                level: LogLevel::Error,
                                message: "Tagging failed: API error".to_string(),
                                timestamp: chrono::Local::now(),
                            }))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(ProgressMessage::Log(LogMessage {
                                level: LogLevel::Error,
                                message: format!("Tagging failed: {}", e),
                                timestamp: chrono::Local::now(),
                            }))
                            .await;
                    }
                }
            }

            processed += chunk.len();
            let progress = (processed as f32 / total_emails as f32) * 100.0;
            let _ = tx.send(ProgressMessage::Progress(progress)).await;
            let _ = tx
                .send(ProgressMessage::Status(format!(
                    "Processing... {}/{}",
                    processed, total_emails
                )))
                .await;

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    let success = 0;
    let failed = 0;
    let _ = tx.send(ProgressMessage::Finished { success, failed }).await;
}

impl eframe::App for IntercomTagsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_progress();

        egui::SidePanel::left("left_panel")
            .default_width(400.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Intercom Tags Manager");
                    ui.separator();

                    ui.group(|ui| {
                        ui.label(RichText::new("Configuration").font(FontId::default()).size(16.0).strong());
                        ui.add_space(5.0);

                        ui.label("API Token:");
                        let token_response = ui.add(
                            egui::TextEdit::singleline(&mut self.config.token)
                                .password(true)
                                .desired_width(f32::INFINITY),
                        );
                        if token_response.changed() {
                            self.config_changed = true;
                        }

                        ui.label("Retries:");
                        if ui.add(egui::Slider::new(&mut self.config.retries, 1..=10)).changed() {
                            self.config_changed = true;
                        }

                        ui.label("Concurrency:");
                        if ui.add(egui::Slider::new(&mut self.config.concurrency, 1..=20)).changed() {
                            self.config_changed = true;
                        }

                        if self.config_changed {
                            if ui.button("Save Config").clicked() {
                                if let Err(e) = self.config.save() {
                                    self.log(LogLevel::Error, format!("Failed to save config: {}", e));
                                } else {
                                    self.config_changed = false;
                                    self.log(LogLevel::Success, "Config saved");
                                }
                            }
                        }
                    });

                    ui.add_space(10.0);

                    ui.group(|ui| {
                        ui.label(RichText::new("Input").font(FontId::default()).size(16.0).strong());
                        ui.add_space(5.0);

                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.input_mode, InputMode::File, "File");
                            ui.selectable_value(&mut self.input_mode, InputMode::Manual, "Manual");
                        });

                        ui.add_space(10.0);

                        match self.input_mode {
                            InputMode::File => {
                                if ui.button("Select File (CSV/XLSX)").clicked() {
                                    self.select_file();
                                }

                                if let Some(path) = &self.selected_file {
                                    ui.label(format!("Selected: {}", path.display()));
                                    ui.label(format!("Records: {}", self.file_emails.len()));
                                }
                            }
                            InputMode::Manual => {
                                ui.label("Tag Name:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.manual_tag)
                                        .desired_width(f32::INFINITY),
                                );

                                ui.label("Emails (one per line):");
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.manual_emails)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(10),
                                );
                            }
                        }
                    });

                    ui.add_space(10.0);

                    let can_start = self.can_start();
                    let button_text = if self.is_running {
                        "Running..."
                    } else {
                        "Start"
                    };

                    ui.add_enabled_ui(can_start, |ui| {
                        if ui
                            .add_sized([f32::INFINITY, 40.0], egui::Button::new(button_text))
                            .clicked()
                        {
                            self.start_execution();
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status_message).strong());
                if self.is_running {
                    ui.spinner();
                }
            });

            if self.progress > 0.0 {
                let progress_color = if self.progress >= 100.0 {
                    Color32::from_rgb(0, 200, 0)
                } else {
                    Color32::from_rgb(0, 150, 255)
                };
                let progress_bar = egui::ProgressBar::new(self.progress / 100.0)
                    .text(format!("{:.1}%", self.progress))
                    .desired_width(f32::INFINITY)
                    .fill(progress_color);
                ui.add(progress_bar);
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Success: {}", self.success_count))
                        .color(Color32::from_rgb(0, 200, 0)),
                );
                ui.label(
                    RichText::new(format!("Failed: {}", self.failed_count))
                        .color(Color32::from_rgb(200, 0, 0)),
                );
            });

            ui.separator();

            ui.heading("Log");
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for log in &self.log_messages {
                        let color = match log.level {
                            LogLevel::Info => Color32::from_rgb(150, 150, 150),
                            LogLevel::Warn => Color32::from_rgb(255, 200, 0),
                            LogLevel::Error => Color32::from_rgb(255, 100, 100),
                            LogLevel::Success => Color32::from_rgb(100, 255, 100),
                        };

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(log.timestamp.format("%H:%M:%S").to_string())
                                    .color(Color32::GRAY)
                                    .size(10.0),
                            );
                            ui.label(RichText::new(&log.message).color(color));
                        });
                    }

                    if !self.log_messages.is_empty() {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }
                });
        });

        if self.is_running {
            ctx.request_repaint();
        }
    }
}
