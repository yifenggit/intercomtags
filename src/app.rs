use crate::config::{Config, IntercomRegion};
use crate::file_parser::{self, EmailWithTag};
use crate::intercom::{IntercomClient, TagResult};
use eframe::egui;
use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

// 主题颜色
const PRIMARY_COLOR: Color32 = Color32::from_rgb(99, 102, 241);
const SUCCESS_COLOR: Color32 = Color32::from_rgb(34, 197, 94);
const ERROR_COLOR: Color32 = Color32::from_rgb(239, 68, 68);
const WARNING_COLOR: Color32 = Color32::from_rgb(245, 158, 11);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(17, 24, 39);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(107, 114, 128);

pub struct IntercomTagsApp {
    config: Config,
    config_changed: bool,
    input_mode: InputMode,
    selected_file: Option<PathBuf>,
    file_emails: Vec<EmailWithTag>,
    manual_emails: String,
    manual_tag: String,
    is_running: bool,
    should_stop: Arc<AtomicBool>,
    progress: f32,
    status_message: String,
    results: Vec<TagResult>,
    success_count: usize,
    failed_count: usize,
    log_messages: Vec<LogMessage>,
    pending_logs: Vec<LogMessage>,
    pending_results: Vec<TagResult>,
    pending_commands: Vec<UiCommand>,
    rt: tokio::runtime::Runtime,
    progress_rx: Option<mpsc::Receiver<ProgressMessage>>,
    processed_emails: HashSet<String>,
}

#[derive(Debug, Clone)]
enum UiCommand {
    ClearLogs,
    ResetExecution,
    ExportResults,
    SelectFile,
    StopExecution,
    StartExecution,
    SaveConfig,
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
            should_stop: Arc::new(AtomicBool::new(false)),
            progress: 0.0,
            status_message: "准备就绪".to_string(),
            results: Vec::new(),
            success_count: 0,
            failed_count: 0,
            log_messages: Vec::new(),
            pending_logs: Vec::new(),
            pending_results: Vec::new(),
            pending_commands: Vec::new(),
            rt,
            progress_rx: None,
            processed_emails: HashSet::new(),
        }
    }

    fn parse_manual_emails(&self) -> Vec<EmailWithTag> {
        self.manual_emails.lines().filter_map(|line| {
            let email = line.trim();
            if email.contains('@') && !email.is_empty() {
                Some(EmailWithTag {
                    email: email.to_string(),
                    tag: if self.manual_tag.is_empty() { None } else { Some(self.manual_tag.clone()) },
                })
            } else {
                None
            }
        }).collect()
    }

    fn can_start(&self) -> bool {
        !self.is_running && !self.config.token.is_empty() && match self.input_mode {
            InputMode::File => !self.file_emails.is_empty(),
            InputMode::Manual => !self.manual_emails.trim().is_empty() && !self.manual_tag.is_empty(),
        }
    }

    fn update_progress(&mut self) {
        if let Some(rx) = &mut self.progress_rx {
            for _ in 0..10 {
                match rx.try_recv() {
                    Ok(msg) => {
                        match msg {
                            ProgressMessage::Log(log) => self.pending_logs.push(log),
                            ProgressMessage::Result(result) => self.pending_results.push(result),
                            ProgressMessage::Progress(p) => self.progress = p,
                            ProgressMessage::Status(s) => self.status_message = s,
                            ProgressMessage::Finished { success, failed } => {
                                self.is_running = false;
                                self.success_count = success;
                                self.failed_count = failed;
                                self.progress = 100.0;
                                self.status_message = format!("执行完成！成功: {}, 失败: {}", success, failed);
                            }
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.progress_rx = None;
                        break;
                    }
                }
            }
        }
    }

    fn apply_pending(&mut self) {
        let commands: Vec<UiCommand> = self.pending_commands.drain(..).collect();
        for cmd in commands {
            match cmd {
                UiCommand::ClearLogs => {
                    self.log_messages.clear();
                }
                UiCommand::ResetExecution => {
                    self.processed_emails.clear();
                    self.results.clear();
                    self.pending_results.clear();
                    self.pending_logs.clear();
                    self.success_count = 0;
                    self.failed_count = 0;
                    self.progress = 0.0;
                    self.status_message = "已重置，准备就绪".to_string();
                    self.log_messages.clear();
                }
                UiCommand::ExportResults => self.do_export_results(),
                UiCommand::SelectFile => self.do_select_file(),
                UiCommand::StopExecution => self.do_stop_execution(),
                UiCommand::StartExecution => self.do_start_execution(),
                UiCommand::SaveConfig => {
                    if let Err(e) = self.config.save() {
                        self.pending_logs.push(LogMessage {
                            level: LogLevel::Error,
                            message: format!("保存配置失败: {}", e),
                            timestamp: chrono::Local::now(),
                        });
                    } else {
                        self.config_changed = false;
                        self.pending_logs.push(LogMessage {
                            level: LogLevel::Success,
                            message: "配置已保存".to_string(),
                            timestamp: chrono::Local::now(),
                        });
                    }
                }
            }
        }

        for log in self.pending_logs.drain(..) {
            self.log_messages.push(log);
        }
        while self.log_messages.len() > 200 {
            self.log_messages.remove(0);
        }

        for result in self.pending_results.drain(..) {
            self.processed_emails.insert(result.email.clone());
            if result.success { self.success_count += 1; } else { self.failed_count += 1; }
            self.results.push(result);
        }
    }

    fn do_stop_execution(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.is_running = false;
        self.status_message = format!("已暂停 (已处理 {} 个)", self.processed_emails.len());
        self.log_messages.push(LogMessage {
            level: LogLevel::Warn,
            message: format!("用户暂停执行，已处理 {} 个邮箱", self.processed_emails.len()),
            timestamp: chrono::Local::now(),
        });
    }

    fn do_select_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV files", &["csv"])
            .add_filter("Excel files", &["xlsx", "xls"])
            .pick_file()
        {
            match file_parser::parse_file(&path) {
                Ok(emails) => {
                    self.selected_file = Some(path.clone());
                    self.file_emails = emails.clone();
                    self.processed_emails.clear();
                    self.results.clear();
                    self.success_count = 0;
                    self.failed_count = 0;
                    self.progress = 0.0;
                    self.log_messages.push(LogMessage {
                        level: LogLevel::Success,
                        message: format!("成功加载文件: {} ({} 条记录)", path.display(), emails.len()),
                        timestamp: chrono::Local::now(),
                    });
                    self.status_message = format!("已加载 {} 条记录", emails.len());
                }
                Err(e) => {
                    self.log_messages.push(LogMessage {
                        level: LogLevel::Error,
                        message: format!("解析文件失败: {}", e),
                        timestamp: chrono::Local::now(),
                    });
                    self.status_message = "文件解析失败".to_string();
                }
            }
        }
    }

    fn do_export_results(&mut self) {
        use rust_xlsxwriter::{Workbook, XlsxError};
        
        if self.results.is_empty() {
            self.log_messages.push(LogMessage {
                level: LogLevel::Warn,
                message: "没有可导出的结果".to_string(),
                timestamp: chrono::Local::now(),
            });
            return;
        }

        let default_filename = format!("intercom_results_{}.xlsx", chrono::Local::now().format("%Y%m%d_%H%M%S"));
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Excel files", &["xlsx"])
            .set_file_name(&default_filename)
            .save_file()
        else { return; };

        let result: Result<(), XlsxError> = (|| {
            let mut workbook = Workbook::new();
            let worksheet = workbook.add_worksheet();
            worksheet.set_column_width(0, 40)?;
            worksheet.set_column_width(1, 12)?;
            worksheet.set_column_width(2, 50)?;

            let header_format = rust_xlsxwriter::Format::new()
                .set_bold()
                .set_background_color(rust_xlsxwriter::Color::RGB(0x4F46E5))
                .set_font_color(rust_xlsxwriter::Color::White);

            worksheet.write_with_format(0, 0, "Email", &header_format)?;
            worksheet.write_with_format(0, 1, "Status", &header_format)?;
            worksheet.write_with_format(0, 2, "Message", &header_format)?;

            let success_format = rust_xlsxwriter::Format::new()
                .set_background_color(rust_xlsxwriter::Color::RGB(0xDCFCE7));
            let failed_format = rust_xlsxwriter::Format::new()
                .set_background_color(rust_xlsxwriter::Color::RGB(0xFEE2E2));

            for (row, result) in self.results.iter().enumerate() {
                let row = (row + 1) as u32;
                let status = if result.success { "Success" } else { "Failed" };
                let format = if result.success { &success_format } else { &failed_format };
                worksheet.write(row, 0, &result.email)?;
                worksheet.write_with_format(row, 1, status, format)?;
                worksheet.write(row, 2, &result.message)?;
            }
            workbook.save(&path)?;
            Ok(())
        })();

        match result {
            Ok(()) => self.log_messages.push(LogMessage {
                level: LogLevel::Success,
                message: format!("结果已导出到: {}", path.display()),
                timestamp: chrono::Local::now(),
            }),
            Err(e) => self.log_messages.push(LogMessage {
                level: LogLevel::Error,
                message: format!("导出失败: {}", e),
                timestamp: chrono::Local::now(),
            }),
        }
    }

    fn do_start_execution(&mut self) {
        if self.config_changed {
            if let Err(e) = self.config.save() {
                self.log_messages.push(LogMessage {
                    level: LogLevel::Error,
                    message: format!("保存配置失败: {}", e),
                    timestamp: chrono::Local::now(),
                });
            } else {
                self.config_changed = false;
            }
        }

        let all_emails: Vec<EmailWithTag> = match self.input_mode {
            InputMode::File => self.file_emails.clone(),
            InputMode::Manual => self.parse_manual_emails(),
        };

        if all_emails.is_empty() {
            self.log_messages.push(LogMessage {
                level: LogLevel::Error,
                message: "没有有效的邮箱地址".to_string(),
                timestamp: chrono::Local::now(),
            });
            return;
        }

        let emails: Vec<EmailWithTag> = all_emails.into_iter()
            .filter(|item| !self.processed_emails.contains(&item.email))
            .collect();

        if emails.is_empty() {
            self.log_messages.push(LogMessage {
                level: LogLevel::Warn,
                message: "所有邮箱已处理完成，无需继续".to_string(),
                timestamp: chrono::Local::now(),
            });
            self.status_message = "所有邮箱已处理完成".to_string();
            return;
        }

        self.should_stop.store(false, Ordering::SeqCst);

        self.is_running = true;
        let already_processed = self.processed_emails.len();
        let already_success = self.success_count;
        let already_failed = self.failed_count;
        self.status_message = format!("继续执行... (已完成 {} 个)", already_processed);

        let total_to_process = emails.len();
        let manual_tag = self.manual_tag.clone();
        let input_mode = self.input_mode.clone();

        let mut tag_groups: HashMap<String, Vec<String>> = HashMap::new();
        for item in emails {
            let tag = item.tag.clone().unwrap_or_else(|| {
                if input_mode == InputMode::Manual { manual_tag.clone() } else { "default".to_string() }
            });
            tag_groups.entry(tag).or_default().push(item.email);
        }

        let token = self.config.token.clone();
        let retries = self.config.retries;
        let concurrency = self.config.concurrency;
        let region = self.config.region.clone();
        let should_stop = Arc::clone(&self.should_stop);

        let (tx, rx) = mpsc::channel(100);
        self.progress_rx = Some(rx);

        self.log_messages.push(LogMessage {
            level: LogLevel::Info,
            message: format!("开始处理 {} 个邮箱（共 {} 个标签）[{}服务器]", total_to_process, tag_groups.len(), region.as_str()),
            timestamp: chrono::Local::now(),
        });

        self.rt.spawn(async move {
            process_tags(tag_groups, token, retries, concurrency, region, tx, already_processed, already_success, already_failed, should_stop).await;
        });
    }
}

impl eframe::App for IntercomTagsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_progress();

        egui::SidePanel::left("left_panel")
            .default_width(380.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(20.0);
                    
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Intercom Tags").size(28.0).strong().color(PRIMARY_COLOR));
                        ui.label(RichText::new("批量标签管理工具").size(14.0).color(TEXT_SECONDARY));
                    });
                    ui.add_space(20.0);

                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Settings ").size(18.0));
                            ui.label(RichText::new("配置").size(18.0).strong());
                        });
                        ui.add_space(15.0);

                        ui.label(RichText::new("服务器区域").size(13.0).color(TEXT_SECONDARY));
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            let regions = [
                                (IntercomRegion::US, "🇺🇸 美国", PRIMARY_COLOR),
                                (IntercomRegion::EU, "🇪🇺 欧洲", Color32::from_rgb(59, 130, 246)),
                                (IntercomRegion::AU, "🇦🇺 澳洲", Color32::from_rgb(16, 185, 129)),
                            ];
                            for (region, label, color) in regions {
                                let is_selected = self.config.region == region;
                                let bg_color = if is_selected { color } else { Color32::from_rgb(229, 231, 235) };
                                let text_color = if is_selected { Color32::WHITE } else { TEXT_PRIMARY };
                                
                                if ui.add(
                                    egui::Button::new(RichText::new(label).size(12.0).color(text_color))
                                        .fill(bg_color)
                                        .corner_radius(CornerRadius::same(6))
                                        .min_size(Vec2::new(70.0, 32.0))
                                ).clicked() {
                                    self.config.region = region;
                                    self.config_changed = true;
                                }
                            }
                        });
                        ui.add_space(15.0);

                        ui.label(RichText::new("API Token").size(13.0).color(TEXT_SECONDARY));
                        ui.add_space(5.0);
                        let token_response = ui.add(
                            egui::TextEdit::singleline(&mut self.config.token)
                                .password(true)
                                .desired_width(f32::INFINITY)
                                .margin(Vec2::new(12.0, 10.0))
                        );
                        if token_response.changed() { self.config_changed = true; }
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("重试次数").size(13.0).color(TEXT_SECONDARY));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}", self.config.retries)).size(14.0).strong());
                            });
                        });
                        if ui.add(egui::Slider::new(&mut self.config.retries, 1..=10).show_value(false)).changed() {
                            self.config_changed = true;
                        }
                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("并发数").size(13.0).color(TEXT_SECONDARY));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}", self.config.concurrency)).size(14.0).strong());
                            });
                        });
                        if ui.add(egui::Slider::new(&mut self.config.concurrency, 1..=10).show_value(false)).changed() {
                            self.config_changed = true;
                        }
                        ui.label(RichText::new("注意: 建议设置为 2-4，过高可能触发限流")
                            .size(11.0).color(WARNING_COLOR));
                        ui.add_space(15.0);

                        if self.config_changed {
                            if ui.add(
                                egui::Button::new(RichText::new("保存配置").size(14.0).strong())
                                    .fill(PRIMARY_COLOR)
                                    .corner_radius(CornerRadius::same(8))
                                    .min_size(Vec2::new(ui.available_width(), 40.0))
                            ).clicked() {
                                self.pending_commands.push(UiCommand::SaveConfig);
                            }
                        }
                    });

                    ui.add_space(20.0);

                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Input ").size(18.0));
                            ui.label(RichText::new("输入方式").size(18.0).strong());
                        });
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            let modes = [
                                (InputMode::File, "文件 导入"),
                                (InputMode::Manual, "键盘 手动输入"),
                            ];
                            for (mode, label) in modes {
                                let is_selected = self.input_mode == mode;
                                let bg_color = if is_selected { PRIMARY_COLOR } else { Color32::from_rgb(229, 231, 235) };
                                let text_color = if is_selected { Color32::WHITE } else { TEXT_PRIMARY };
                                
                                if ui.add(
                                    egui::Button::new(RichText::new(label).size(13.0).color(text_color))
                                        .fill(bg_color)
                                        .corner_radius(CornerRadius::same(6))
                                        .min_size(Vec2::new(120.0, 36.0))
                                ).clicked() {
                                    self.input_mode = mode;
                                }
                            }
                        });

                        ui.add_space(15.0);

                        match self.input_mode {
                            InputMode::File => {
                                if ui.add(
                                    egui::Button::new(RichText::new("选择文件 (CSV/XLSX)").size(14.0))
                                        .stroke(Stroke::new(2.0, PRIMARY_COLOR))
                                        .corner_radius(CornerRadius::same(8))
                                        .min_size(Vec2::new(ui.available_width(), 44.0))
                                ).clicked() {
                                    self.pending_commands.push(UiCommand::SelectFile);
                                }

                                if let Some(path) = &self.selected_file {
                                    ui.add_space(10.0);
                                    ui.group(|ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("[文件]").size(13.0));
                                            ui.vertical(|ui| {
                                                ui.label(RichText::new(path.file_name().unwrap_or_default().to_string_lossy().to_string()).size(13.0).strong());
                                                ui.label(RichText::new(format!("{} 条记录", self.file_emails.len())).size(11.0).color(SUCCESS_COLOR));
                                            });
                                        });
                                    });
                                }
                            }
                            InputMode::Manual => {
                                ui.label(RichText::new("标签名称").size(13.0).color(TEXT_SECONDARY));
                                ui.add_space(5.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.manual_tag)
                                        .desired_width(f32::INFINITY)
                                        .margin(Vec2::new(12.0, 10.0))
                                );
                                ui.add_space(10.0);
                                
                                ui.label(RichText::new("邮箱列表（每行一个）").size(13.0).color(TEXT_SECONDARY));
                                ui.add_space(5.0);
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.manual_emails)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(8)
                                        .margin(Vec2::new(12.0, 10.0))
                                );
                            }
                        }
                    });

                    ui.add_space(20.0);

                    if self.is_running {
                        if ui.add(
                            egui::Button::new(RichText::new("停止执行").size(16.0).strong())
                                .fill(ERROR_COLOR)
                                .corner_radius(CornerRadius::same(10))
                                .min_size(Vec2::new(ui.available_width(), 48.0))
                        ).clicked() {
                            self.pending_commands.push(UiCommand::StopExecution);
                        }
                    } else {
                        let can_start = self.can_start();
                        let button_color = if can_start { PRIMARY_COLOR } else { Color32::from_rgb(209, 213, 219) };
                        
                        ui.add_enabled_ui(can_start, |ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("开始执行").size(16.0).strong())
                                    .fill(button_color)
                                    .corner_radius(CornerRadius::same(10))
                                    .min_size(Vec2::new(ui.available_width(), 48.0))
                            ).clicked() {
                                self.pending_commands.push(UiCommand::StartExecution);
                            }
                        });
                    }

                    ui.add_space(20.0);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::vec2(12.0, 12.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&self.status_message).size(15.0).strong());
                        if self.is_running {
                            ui.spinner();
                        }
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.horizontal(|ui| {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(254, 226, 226))
                                    .corner_radius(CornerRadius::same(16))
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(format!("失败 {}", self.failed_count)).size(14.0).strong().color(ERROR_COLOR));
                                    });
                                
                                ui.add_space(10.0);
                                
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(220, 252, 231))
                                    .corner_radius(CornerRadius::same(16))
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(format!("成功 {}", self.success_count)).size(14.0).strong().color(SUCCESS_COLOR));
                                    });
                            });
                        });
                    });

                    if self.progress > 0.0 {
                        ui.add_space(10.0);
                        let progress_color = if self.progress >= 100.0 { SUCCESS_COLOR } else { PRIMARY_COLOR };
                        let progress_bar = egui::ProgressBar::new(self.progress / 100.0)
                            .text(format!("{:.0}%", self.progress))
                            .desired_width(ui.available_width())
                            .fill(progress_color)
                            .corner_radius(CornerRadius::same(4));
                        ui.add(progress_bar);
                    }
                });

            ui.add_space(15.0);

            egui::Frame::NONE
                .inner_margin(egui::vec2(12.0, 12.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(ui.available_height());

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("执行日志").size(16.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let export_btn = ui.add_enabled(
                                !self.results.is_empty() && !self.is_running,
                                egui::Button::new("导出结果")
                            );
                            if export_btn.clicked() {
                                self.pending_commands.push(UiCommand::ExportResults);
                            }

                            ui.add_space(8.0);

                            let reset_btn = ui.add_enabled(
                                !self.processed_emails.is_empty() && !self.is_running,
                                egui::Button::new("重置")
                            );
                            if reset_btn.clicked() {
                                self.pending_commands.push(UiCommand::ResetExecution);
                            }

                            ui.add_space(8.0);

                            if ui.button("清空日志").clicked() {
                                self.pending_commands.push(UiCommand::ClearLogs);
                            }
                        });
                    });

                    ui.add_space(10.0);

                    egui::ScrollArea::vertical()
                        .id_salt("log_scroll")
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for log in &self.log_messages {
                                let color = match log.level {
                                    LogLevel::Info => TEXT_SECONDARY,
                                    LogLevel::Warn => WARNING_COLOR,
                                    LogLevel::Error => ERROR_COLOR,
                                    LogLevel::Success => SUCCESS_COLOR,
                                };
                                ui.label(RichText::new(format!("{} {}", 
                                    log.timestamp.format("%H:%M:%S"),
                                    log.message
                                )).size(12.0).color(color));
                            }
                        });
                });
        });

        if self.is_running {
            ctx.request_repaint();
        }

        self.apply_pending();
    }
}

async fn process_tags(
    tag_groups: HashMap<String, Vec<String>>,
    token: String,
    retries: u32,
    concurrency: u32,
    region: IntercomRegion,
    tx: mpsc::Sender<ProgressMessage>,
    already_processed: usize,
    already_success: usize,
    already_failed: usize,
    should_stop: Arc<AtomicBool>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    use tokio::time::{interval, Duration};

    let client = IntercomClient::new(token, retries, &region);
    let total_emails: usize = tag_groups.values().map(|v| v.len()).sum();
    let processed = StdArc::new(AtomicUsize::new(0));
    let total_success = StdArc::new(AtomicUsize::new(already_success));
    let total_failed = StdArc::new(AtomicUsize::new(already_failed));
    let rate_limiter = StdArc::new(tokio::sync::Mutex::new(interval(Duration::from_millis(125))));

    for (tag_name, emails) in tag_groups {
        // 检查是否停止
        if should_stop.load(Ordering::SeqCst) {
            let _ = tx.send(ProgressMessage::Log(LogMessage {
                level: LogLevel::Warn,
                message: "用户已停止执行".to_string(),
                timestamp: chrono::Local::now(),
            })).await;
            let _ = tx.send(ProgressMessage::Finished {
                success: total_success.load(Ordering::Relaxed),
                failed: total_failed.load(Ordering::Relaxed),
            }).await;
            return;
        }
        let _ = tx.send(ProgressMessage::Log(LogMessage {
            level: LogLevel::Info,
            message: format!("开始处理标签: {} ({} 个邮箱)", tag_name, emails.len()),
            timestamp: chrono::Local::now(),
        })).await;

        let tag = match client.get_or_create_tag(&tag_name).await {
            Ok(tag) => {
                let _ = tx.send(ProgressMessage::Log(LogMessage {
                    level: LogLevel::Success,
                    message: format!("标签已就绪: {} (ID: {})", tag.name, tag.id),
                    timestamp: chrono::Local::now(),
                })).await;
                tag
            }
            Err(e) => {
                let _ = tx.send(ProgressMessage::Log(LogMessage {
                    level: LogLevel::Error,
                    message: format!("获取/创建标签失败: {}", e),
                    timestamp: chrono::Local::now(),
                })).await;
                continue;
            }
        };

        let _ = tx.send(ProgressMessage::Log(LogMessage {
            level: LogLevel::Info,
            message: format!("正在搜索 {} 个联系人...", emails.len()),
            timestamp: chrono::Local::now(),
        })).await;

        use futures::stream::{self, StreamExt};

        let processed = StdArc::clone(&processed);
        let total_success = StdArc::clone(&total_success);
        let total_failed = StdArc::clone(&total_failed);
        let should_stop = Arc::clone(&should_stop);

        let _: Vec<_> = stream::iter(emails.clone())
            .map(|email| {
                let client = client.clone();
                let tag_id = tag.id.clone();
                let tx = tx.clone();
                let processed = StdArc::clone(&processed);
                let total_success = StdArc::clone(&total_success);
                let total_failed = StdArc::clone(&total_failed);
                let rate_limiter = StdArc::clone(&rate_limiter);
                let should_stop = Arc::clone(&should_stop);
                async move {
                    // 检查是否停止
                    if should_stop.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    {
                        let mut limiter = rate_limiter.lock().await;
                        limiter.tick().await;
                    }
                    
                    // 再次检查停止
                    if should_stop.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    let contact_id = match client.search_contact(&email).await {
                        Ok(Some(contact)) => Some(contact.id),
                        Ok(None) => {
                            let _ = tx.send(ProgressMessage::Log(LogMessage {
                                level: LogLevel::Error,
                                message: format!("[失败] {} - 未找到联系人", email),
                                timestamp: chrono::Local::now(),
                            })).await;
                            let _ = tx.send(ProgressMessage::Result(TagResult {
                                email: email.clone(),
                                success: false,
                                message: "未找到联系人".to_string(),
                            })).await;
                            total_failed.fetch_add(1, Ordering::Relaxed);
                            None
                        }
                        Err(e) => {
                            let _ = tx.send(ProgressMessage::Log(LogMessage {
                                level: LogLevel::Error,
                                message: format!("[失败] {} - 搜索失败: {}", email, e),
                                timestamp: chrono::Local::now(),
                            })).await;
                            let _ = tx.send(ProgressMessage::Result(TagResult {
                                email: email.clone(),
                                success: false,
                                message: format!("搜索失败: {}", e),
                            })).await;
                            total_failed.fetch_add(1, Ordering::Relaxed);
                            None
                        }
                    };

                    if let Some(cid) = contact_id {
                        {
                            let mut limiter = rate_limiter.lock().await;
                            limiter.tick().await;
                        }
                        
                        // 打标签前检查停止
                        if should_stop.load(Ordering::SeqCst) {
                            return;
                        }
                        
                        match client.tag_contact_single(&cid, &tag_id).await {
                            Ok(_) => {
                                let _ = tx.send(ProgressMessage::Log(LogMessage {
                                    level: LogLevel::Success,
                                    message: format!("[成功] {} - 标签添加成功", email),
                                    timestamp: chrono::Local::now(),
                                })).await;
                                let _ = tx.send(ProgressMessage::Result(TagResult {
                                    email: email.clone(),
                                    success: true,
                                    message: "标签添加成功".to_string(),
                                })).await;
                                total_success.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                let _ = tx.send(ProgressMessage::Log(LogMessage {
                                    level: LogLevel::Error,
                                    message: format!("[失败] {} - 打标签失败: {}", email, e),
                                    timestamp: chrono::Local::now(),
                                })).await;
                                let _ = tx.send(ProgressMessage::Result(TagResult {
                                    email: email.clone(),
                                    success: false,
                                    message: format!("打标签失败: {}", e),
                                })).await;
                                total_failed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }

                    let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
                    let batch_progress = (current as f32 / total_emails as f32) * 100.0;
                    let overall_processed = already_processed + current;
                    let _ = tx.send(ProgressMessage::Progress(batch_progress)).await;
                    let _ = tx.send(ProgressMessage::Status(format!("处理中... {}/{}", overall_processed, already_processed + total_emails))).await;
                }
            })
            .buffer_unordered((concurrency as usize).min(4))
            .collect()
            .await;

        let _ = tx.send(ProgressMessage::Log(LogMessage {
            level: LogLevel::Success,
            message: format!("标签 '{}' 处理完成", tag_name),
            timestamp: chrono::Local::now(),
        })).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    let _ = tx.send(ProgressMessage::Finished {
        success: total_success.load(Ordering::Relaxed),
        failed: total_failed.load(Ordering::Relaxed),
    }).await;
}
