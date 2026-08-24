#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod launcher;

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::Duration,
};

use chrono::{Local, NaiveTime};
use eframe::egui;
use pontemesh_sdk_core::CancellationToken;
use sysinfo::System;

use config::LauncherConfig;
use launcher::{FragmentLogEntry, GameLauncher, InstallReport, ProgressMessage};

fn main() -> eframe::Result<()> {
    let icon_data = include_bytes!("../assets/icon.jpg");
    let image = image::load_from_memory(icon_data).expect("Failed to load icon").into_rgba8();
    let (width, height) = image.dimensions();
    let icon = egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 620.0])
            .with_min_inner_size([720.0, 520.0])
            .with_title("Ponte Mesh - Test Launcher")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "Ponte Mesh Test Launcher",
        options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AppLanguage {
    English,
    Portuguese,
}

fn tr(lang: AppLanguage, en: &str, pt: &str) -> String {
    match lang {
        AppLanguage::English => en.to_string(),
        AppLanguage::Portuguese => pt.to_string(),
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AppTab {
    Test,
    Dashboard,
    ScheduleLoop,
    Settings,
}

#[derive(Debug, PartialEq, Eq)]
enum LauncherState {
    Idle,
    ScheduledWaiting,
    Downloading,
    CycleCoolingDown,
    Ready,
    Error,
}

struct LauncherApp {
    config: LauncherConfig,
    config_path: PathBuf,
    language: AppLanguage,
    active_tab: AppTab,
    state: LauncherState,
    cancellation: Option<CancellationToken>,
    rx: Option<Receiver<ProgressMessage>>,
    status_text: String,
    progress_percent: f32,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bps: u64,
    current_file: String,
    report: Option<InstallReport>,
    reports_history: Vec<(usize, String, InstallReport)>,
    fragment_logs: Vec<FragmentLogEntry>,
    error_message: Option<String>,
    schedule_time_str: String,
    schedule_active: bool,
    loop_enabled: bool,
    loop_total_cycles: u32,
    loop_current_cycle: u32,
    loop_cooldown_seconds: u64,
    loop_auto_clean: bool,
    cycle_cooldown_remaining: u64,
    last_tick: std::time::Instant,
    input_origin_url: String,
    input_token: String,
    input_bucket: String,
    input_manifest: String,
    input_install_dir: String,
    input_cache_dir: String,
    input_p2p_listen: String,
    input_p2p_announce: String,
    input_seed_seconds: String,
    settings_feedback: Option<(bool, String)>,
    connection_test_feedback: Option<(bool, String)>,
    export_feedback: Option<String>,
    peer_bytes: std::collections::HashMap<String, u64>,
    sys: System,
    cpu_usage: f32,
    memory_used: u64,
    memory_total: u64,
}

impl LauncherApp {
    fn new() -> Self {
        let config_path = env::var_os("PONTEMESH_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("launcher.toml"));

        let config = LauncherConfig::load(&config_path).unwrap_or_default();

        let input_origin_url = config.origin_url.clone();
        let input_token = config.application_token.clone();
        let input_bucket = config.release_bucket.clone();
        let input_manifest = config.release_manifest_key.clone();
        let input_install_dir = config.install_directory.clone();
        let input_cache_dir = config.cache_directory.clone();
        let input_p2p_listen = config.p2p_listen_address.clone().unwrap_or_default();
        let input_p2p_announce = config.p2p_announce_address.clone().unwrap_or_default();
        let input_seed_seconds = config.seed_seconds.to_string();

        let default_schedule_time = (Local::now() + chrono::Duration::seconds(30))
            .format("%H:%M:%S")
            .to_string();

        Self {
            config,
            config_path,
            language: AppLanguage::Portuguese,
            active_tab: AppTab::Test,
            state: LauncherState::Idle,
            cancellation: None,
            rx: None,
            status_text: "Pronto para testar o download.".to_string(),
            progress_percent: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bps: 0,
            current_file: String::new(),
            report: None,
            reports_history: Vec::new(),
            fragment_logs: Vec::new(),
            error_message: None,
            schedule_time_str: default_schedule_time,
            schedule_active: false,
            loop_enabled: false,
            loop_total_cycles: 3,
            loop_current_cycle: 1,
            loop_cooldown_seconds: 3,
            loop_auto_clean: true,
            cycle_cooldown_remaining: 0,
            last_tick: std::time::Instant::now(),
            input_origin_url,
            input_token,
            input_bucket,
            input_manifest,
            input_install_dir,
            input_cache_dir,
            input_p2p_listen,
            input_p2p_announce,
            input_seed_seconds,
            settings_feedback: None,
            connection_test_feedback: None,
            export_feedback: None,
            peer_bytes: std::collections::HashMap::new(),
            sys: System::new_all(),
            cpu_usage: 0.0,
            memory_used: 0,
            memory_total: 0,
        }
    }

    fn sync_inputs_from_config(&mut self) {
        self.input_origin_url = self.config.origin_url.clone();
        self.input_token = self.config.application_token.clone();
        self.input_bucket = self.config.release_bucket.clone();
        self.input_manifest = self.config.release_manifest_key.clone();
        self.input_install_dir = self.config.install_directory.clone();
        self.input_cache_dir = self.config.cache_directory.clone();
        self.input_p2p_listen = self.config.p2p_listen_address.clone().unwrap_or_default();
        self.input_p2p_announce = self.config.p2p_announce_address.clone().unwrap_or_default();
        self.input_seed_seconds = self.config.seed_seconds.to_string();
    }

    fn apply_inputs_to_config(&mut self) -> Result<(), String> {
        self.config.origin_url = self.input_origin_url.trim().to_string();
        self.config.application_token = self.input_token.trim().to_string();
        self.config.release_bucket = self.input_bucket.trim().to_string();
        self.config.release_manifest_key = self.input_manifest.trim().to_string();
        self.config.install_directory = self.input_install_dir.trim().to_string();
        self.config.cache_directory = self.input_cache_dir.trim().to_string();

        self.config.p2p_listen_address = if self.input_p2p_listen.trim().is_empty() {
            None
        } else {
            Some(self.input_p2p_listen.trim().to_string())
        };

        self.config.p2p_announce_address = if self.input_p2p_announce.trim().is_empty() {
            None
        } else {
            Some(self.input_p2p_announce.trim().to_string())
        };

        self.config.seed_seconds = self
            .input_seed_seconds
            .trim()
            .parse::<u64>()
            .map_err(|_| "Seed seconds must be a positive integer".to_string())?;

        self.config.validate()?;
        Ok(())
    }

    fn trigger_start_download(&mut self, is_loop_cycle: bool) {
        if !is_loop_cycle {
            self.loop_current_cycle = 1;
        }

        if let Err(err) = self.apply_inputs_to_config() {
            self.error_message = Some(err);
            self.state = LauncherState::Error;
            return;
        }

        let launcher = GameLauncher::new(self.config.clone());
        if self.loop_enabled && self.loop_auto_clean {
            let _ = launcher.clean_installation_and_cache();
        }

        self.state = LauncherState::Downloading;
        self.status_text = format!("Cycle {}/{} in progress...", self.loop_current_cycle, if self.loop_enabled { self.loop_total_cycles } else { 1 });
        self.progress_percent = 0.0;
        self.downloaded_bytes = 0;
        self.total_bytes = 0;
        self.speed_bps = 0;
        self.current_file = String::new();
        self.error_message = None;
        self.peer_bytes.clear();

        let (tx, rx): (Sender<ProgressMessage>, Receiver<ProgressMessage>) = channel();
        self.rx = Some(rx);
        let cancellation = CancellationToken::default();
        self.cancellation = Some(cancellation.clone());

        let config = self.config.clone();
        thread::spawn(move || {
            let launcher = GameLauncher::new(config);
            match launcher.install_latest(cancellation, tx.clone()) {
                Ok(_) => {}
                Err(e) => {
                    let _ = tx.send(ProgressMessage::Error(e));
                }
            }
        });
    }

    fn cancel_download(&mut self) {
        if let Some(cancel) = &self.cancellation {
            cancel.cancel();
        }
        self.schedule_active = false;
        self.state = LauncherState::Idle;
        self.status_text = "Operation cancelled by user.".to_string();
    }

    fn export_reports_csv(&self, path: &Path) -> Result<(), String> {
        let mut lines = Vec::new();
        lines.push("cycle,timestamp,version,destination,total_bytes,elapsed_ms,bytes_origin,bytes_peer,bytes_replica,fallback_activations,peer_failures,peer_hash_failures,offload_percent".to_string());

        for (cycle, timestamp, report) in &self.reports_history {
            lines.push(format!(
                "{},\"{}\",\"{}\",\"{}\",{},{},{},{},{},{},{},{},{:.2}",
                cycle,
                timestamp,
                report.version,
                report.destination.display(),
                report.bytes,
                report.elapsed_ms,
                report.summary.bytes_from_origin,
                report.summary.bytes_from_peer,
                report.summary.bytes_from_replica,
                report.summary.fallback_activations,
                report.summary.peer_failures,
                report.summary.peer_hash_failures,
                report.offload_percent()
            ));
        }

        if lines.len() == 1 {
            if let Some(report) = &self.report {
                lines.push(format!(
                    "1,\"{}\",\"{}\",\"{}\",{},{},{},{},{},{},{},{},{:.2}",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    report.version,
                    report.destination.display(),
                    report.bytes,
                    report.elapsed_ms,
                    report.summary.bytes_from_origin,
                    report.summary.bytes_from_peer,
                    report.summary.bytes_from_replica,
                    report.summary.fallback_activations,
                    report.summary.peer_failures,
                    report.summary.peer_hash_failures,
                    report.offload_percent()
                ));
            }
        }

        fs::write(path, lines.join("\n")).map_err(|e| format!("Failed to write CSV: {e}"))?;
        Ok(())
    }

    fn test_connection(&mut self) {
        let url = self.input_origin_url.trim().to_string();
        let token = self.input_token.trim().to_string();

        if url.is_empty() {
            self.connection_test_feedback = Some((false, "Origin URL cannot be empty".to_string()));
            return;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();

        match client {
            Ok(http) => {
                let health_url = format!("{}/api/v1/health", url.trim_end_matches('/'));
                let mut req = http.get(&health_url);
                if !token.is_empty() {
                    req = req.header("Authorization", format!("Bearer {token}"));
                }

                match req.send() {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            self.connection_test_feedback = Some((
                                true,
                                format!("Connection successful (HTTP {})", status.as_u16()),
                            ));
                        } else {
                            self.connection_test_feedback = Some((
                                false,
                                format!("Server replied with HTTP {}", status.as_u16()),
                            ));
                        }
                    }
                    Err(e) => {
                        let base_req = http.get(&url).send();
                        match base_req {
                            Ok(resp) => {
                                self.connection_test_feedback = Some((
                                    true,
                                    format!("Reachable (HTTP {})", resp.status().as_u16()),
                                ));
                            }
                            Err(_) => {
                                self.connection_test_feedback = Some((
                                    false,
                                    format!("Connection failed: {e}"),
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                self.connection_test_feedback = Some((false, format!("HTTP Client error: {e}")));
            }
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = std::time::Instant::now();
        let delta = now.duration_since(self.last_tick);
        if delta >= Duration::from_secs(1) {
            self.last_tick = now;
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory();
            self.cpu_usage = self.sys.global_cpu_usage();
            self.memory_used = self.sys.used_memory();
            self.memory_total = self.sys.total_memory();

            if self.state == LauncherState::ScheduledWaiting && self.schedule_active {
                if let Ok(target_time) = NaiveTime::parse_from_str(&self.schedule_time_str, "%H:%M:%S") {
                    let current_time = Local::now().time();
                    if current_time >= target_time && (current_time - target_time).num_seconds() < 5 {
                        self.schedule_active = false;
                        self.trigger_start_download(false);
                    }
                }
            }

            if self.state == LauncherState::CycleCoolingDown {
                if self.cycle_cooldown_remaining > 0 {
                    self.cycle_cooldown_remaining -= 1;
                } else {
                    self.loop_current_cycle += 1;
                    if self.loop_current_cycle <= self.loop_total_cycles {
                        self.trigger_start_download(true);
                    } else {
                        self.state = LauncherState::Ready;
                        self.status_text = format!("All {} test cycles completed successfully!", self.loop_total_cycles);
                    }
                }
            }
        }

        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ProgressMessage::Status(status) => {
                        self.status_text = status;
                    }
                    ProgressMessage::Progress {
                        file,
                        downloaded,
                        total,
                        percent,
                        speed_bytes_per_sec,
                    } => {
                        self.progress_percent = percent as f32 / 100.0;
                        self.downloaded_bytes = downloaded;
                        self.total_bytes = total;
                        self.speed_bps = speed_bytes_per_sec;
                        self.current_file = file;
                    }
                    ProgressMessage::Fragment(log_item) => {
                        *self.peer_bytes.entry(log_item.source.clone()).or_insert(0) += log_item.bytes;
                        
                        if self.fragment_logs.len() > 300 {
                            self.fragment_logs.remove(0);
                        }
                        self.fragment_logs.push(log_item);
                    }
                    ProgressMessage::Done(report) => {
                        self.report = Some(report.clone());
                        let timestamp = Local::now().format("%H:%M:%S").to_string();
                        self.reports_history.push((self.loop_current_cycle as usize, timestamp, report.clone()));

                        if self.loop_enabled && self.loop_current_cycle < self.loop_total_cycles {
                            self.state = LauncherState::CycleCoolingDown;
                            self.cycle_cooldown_remaining = self.loop_cooldown_seconds;
                            self.status_text = format!(
                                "Cycle {} done. Cooling down ({}s) before cycle {}...",
                                self.loop_current_cycle,
                                self.loop_cooldown_seconds,
                                self.loop_current_cycle + 1
                            );
                        } else {
                            self.state = LauncherState::Ready;
                            self.status_text = "Transfer completed successfully.".to_string();
                        }
                        report.print();
                    }
                    ProgressMessage::Error(err) => {
                        self.error_message = Some(err);
                        self.state = LauncherState::Error;
                    }
                }
            }
        }

        if self.state == LauncherState::Downloading
            || self.state == LauncherState::ScheduledWaiting
            || self.state == LauncherState::CycleCoolingDown
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("PONTE MESH");
                ui.label(egui::RichText::new(tr(self.language, "Network Test Launcher", "Testador de Rede (Launcher)")).strong().color(egui::Color32::from_rgb(140, 180, 255)));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let origin_display = if self.config.origin_url.len() > 40 {
                        format!("{}...", &self.config.origin_url[..37])
                    } else {
                        self.config.origin_url.clone()
                    };
                    ui.label(egui::RichText::new(format!("{}: {}", tr(self.language, "Origin", "Origin"), origin_display)).small().weak());
                    
                    ui.separator();
                    if self.language == AppLanguage::English {
                        if ui.button("🇧🇷 PT-BR").clicked() {
                            self.language = AppLanguage::Portuguese;
                        }
                    } else {
                        if ui.button("🇺🇸 EN-US").clicked() {
                            self.language = AppLanguage::English;
                        }
                    }
                });
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, AppTab::Test, tr(self.language, "  Test & Download  ", "  Teste e Download  "));
                ui.selectable_value(&mut self.active_tab, AppTab::Dashboard, tr(self.language, "  Dashboard & Metrics  ", "  Dashboard e Métricas  "));
                ui.selectable_value(&mut self.active_tab, AppTab::ScheduleLoop, tr(self.language, "  Schedule & Loop  ", "  Agendamento e Loop  "));
                ui.selectable_value(&mut self.active_tab, AppTab::Settings, tr(self.language, "  Settings & Paths  ", "  Configurações e Caminhos  "));
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                AppTab::Test => self.render_test_tab(ui),
                AppTab::Dashboard => self.render_dashboard_tab(ui),
                AppTab::ScheduleLoop => self.render_schedule_loop_tab(ui),
                AppTab::Settings => self.render_settings_tab(ui),
            }
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let state_label = match self.state {
                    LauncherState::Idle => egui::RichText::new("● Idle").color(egui::Color32::GRAY),
                    LauncherState::ScheduledWaiting => egui::RichText::new("● Waiting for schedule").color(egui::Color32::YELLOW),
                    LauncherState::Downloading => egui::RichText::new("● Downloading").color(egui::Color32::GREEN),
                    LauncherState::CycleCoolingDown => egui::RichText::new("● Cycle cooldown").color(egui::Color32::LIGHT_BLUE),
                    LauncherState::Ready => egui::RichText::new("● Ready").color(egui::Color32::GREEN),
                    LauncherState::Error => egui::RichText::new("● Error").color(egui::Color32::RED),
                };
                ui.label(state_label);
                ui.separator();
                ui.label(egui::RichText::new(&self.status_text).small());

                if self.state == LauncherState::Downloading && self.speed_bps > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mb_s = self.speed_bps as f64 / (1024.0 * 1024.0);
                        ui.label(egui::RichText::new(format!("{:.2} MB/s", mb_s)).strong().color(egui::Color32::LIGHT_GREEN));
                    });
                }
            });
        });
    }
}

impl LauncherApp {
    fn render_test_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Target Release Information", "Informações do Release Alvo")).strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tr(self.language, "Origin URL:", "URL do Origin:"));
                ui.label(egui::RichText::new(&self.config.origin_url).color(egui::Color32::LIGHT_BLUE));
            });
            ui.horizontal(|ui| {
                ui.label(tr(self.language, "Release Bucket:", "Bucket do Release:"));
                ui.label(egui::RichText::new(&self.config.release_bucket).strong());
                ui.separator();
                ui.label(tr(self.language, "Manifest Key:", "Chave do Manifesto:"));
                ui.label(egui::RichText::new(&self.config.release_manifest_key).strong());
            });
            ui.horizontal(|ui| {
                ui.label(tr(self.language, "Install Path:", "Caminho de Instalação:"));
                ui.label(egui::RichText::new(&self.config.install_directory).small());
            });
        });

        ui.add_space(14.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Execution & Progress", "Execução & Progresso")).strong());
            ui.add_space(8.0);

            let progress_bar = egui::ProgressBar::new(self.progress_percent)
                .show_percentage()
                .animate(self.state == LauncherState::Downloading);
            ui.add(progress_bar);

            ui.add_space(4.0);
            if !self.current_file.is_empty() {
                let down_mb = self.downloaded_bytes as f64 / (1024.0 * 1024.0);
                let tot_mb = self.total_bytes as f64 / (1024.0 * 1024.0);
                ui.label(egui::RichText::new(format!("Current file: {} ({:.2} MB / {:.2} MB)", self.current_file, down_mb, tot_mb)).small());
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                match self.state {
                    LauncherState::Idle | LauncherState::Ready | LauncherState::Error => {
                        if ui.button(egui::RichText::new(tr(self.language, "  Start Download Test  ", "  Iniciar Teste de Download  ")).strong()).clicked() {
                            self.trigger_start_download(false);
                        }
                    }
                    LauncherState::ScheduledWaiting => {
                        if ui.button(egui::RichText::new(tr(self.language, "  Cancel Schedule  ", "  Cancelar Agendamento  ")).color(egui::Color32::YELLOW)).clicked() {
                            self.cancel_download();
                        }
                    }
                    LauncherState::Downloading | LauncherState::CycleCoolingDown => {
                        if ui.button(egui::RichText::new(tr(self.language, "  Cancel  ", "  Cancelar  ")).color(egui::Color32::RED)).clicked() {
                            self.cancel_download();
                        }
                    }
                }

                if ui.button(tr(self.language, "Clean Staging & Cache", "Limpar Cache e Staging")).clicked() {
                    let launcher = GameLauncher::new(self.config.clone());
                    match launcher.clean_installation_and_cache() {
                        Ok(()) => self.status_text = "Cleaned installation directory and fragment cache.".to_string(),
                        Err(e) => self.error_message = Some(e),
                    }
                }
            });
        });

        if let Some(err) = &self.error_message {
            ui.add_space(10.0);
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(50, 15, 15))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(tr(self.language, "Error Occurred", "Ocorreu um Erro")).strong().color(egui::Color32::RED));
                    ui.label(egui::RichText::new(err).color(egui::Color32::LIGHT_RED));
                });
        }

        if let Some(report) = &self.report {
            ui.add_space(12.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(tr(self.language, "Last Transfer Summary", "Resumo da Última Transferência")).strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(format!("Version: {}", report.version));
                    ui.separator();
                    ui.label(format!("Files: {}", report.files));
                    ui.separator();
                    ui.label(format!("Total Size: {:.2} MB", report.bytes as f64 / (1024.0 * 1024.0)));
                    ui.separator();
                    ui.label(format!("Time: {} ms", report.elapsed_ms));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("P2P (LAN): {:.2} MB ({:.1}%)", report.summary.bytes_from_peer as f64 / (1024.0 * 1024.0), report.p2p_percent())).color(egui::Color32::LIGHT_GREEN));
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Origin (Fallback): {:.2} MB ({:.1}%)", report.summary.bytes_from_origin as f64 / (1024.0 * 1024.0), report.origin_percent())).color(egui::Color32::LIGHT_BLUE));
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Origin Offload: {:.1}%", report.offload_percent())).strong().color(egui::Color32::from_rgb(100, 240, 150)));
                });
            });
        }
    }

    fn render_dashboard_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.heading(tr(self.language, "Transfer Metrics & Real-time Logs", "Métricas de Transferência & Logs em Tempo Real"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr(self.language, "Export CSV", "Exportar CSV")).clicked() {
                    let out_path = PathBuf::from("benchmark_metrics.csv");
                    match self.export_reports_csv(&out_path) {
                        Ok(()) => self.export_feedback = Some(format!("Exported to {}", out_path.display())),
                        Err(e) => self.export_feedback = Some(format!("Export failed: {e}")),
                    }
                }
            });
        });

        if let Some(feedback) = &self.export_feedback {
            ui.label(egui::RichText::new(feedback).small().color(egui::Color32::LIGHT_YELLOW));
        }

        ui.add_space(8.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Hardware Monitor (Overhead)", "Monitoramento de Hardware (Overhead)")).strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(tr(self.language, "CPU Usage:", "Uso de CPU:"));
                let cpu_fraction = self.cpu_usage / 100.0;
                ui.add(egui::ProgressBar::new(cpu_fraction).text(format!("{:.1}%", self.cpu_usage)));
                
                ui.separator();

                ui.label(tr(self.language, "RAM Usage:", "Uso de RAM:"));
                let mem_fraction = if self.memory_total > 0 {
                    self.memory_used as f32 / self.memory_total as f32
                } else {
                    0.0
                };
                let mem_used_mb = self.memory_used as f64 / (1024.0 * 1024.0);
                let mem_total_mb = self.memory_total as f64 / (1024.0 * 1024.0);
                ui.add(egui::ProgressBar::new(mem_fraction).text(format!("{:.1} MB / {:.1} MB", mem_used_mb, mem_total_mb)));
            });
        });

        ui.add_space(8.0);

        if let Some(report) = &self.report {
            let p2p_ratio = (report.p2p_percent() / 100.0) as f32;
            let origin_ratio = (report.origin_percent() / 100.0) as f32;

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(tr(self.language, "Bandwidth Distribution (P2P vs Origin Fallback)", "Distribuição de Banda (P2P vs Fallback do Origin)")).strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(tr(self.language, "P2P Mesh Ratio:", "Proporção da Rede P2P:"));
                    ui.add(egui::ProgressBar::new(p2p_ratio).text(format!("{:.1}% P2P", report.p2p_percent())));
                });
                ui.horizontal(|ui| {
                    ui.label(tr(self.language, "Origin Ratio:", "Proporção do Origin:"));
                    ui.add(egui::ProgressBar::new(origin_ratio).text(format!("{:.1}% Origin", report.origin_percent())));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(format!("Fallback Activations: {}", report.summary.fallback_activations));
                    ui.separator();
                    ui.label(format!("Peer Failures: {}", report.summary.peer_failures));
                    ui.separator();
                    ui.label(format!("Fragments (Peer): {}", report.summary.fragments_from_peer));
                    ui.separator();
                    ui.label(format!("Fragments (Origin): {}", report.summary.fragments_from_origin));
                });
            });
        } else {
            ui.label(egui::RichText::new(tr(self.language, "No active report. Run a download test to populate metrics.", "Nenhum relatório ativo. Execute um teste de download para preencher as métricas.")).weak());
        }

        ui.add_space(10.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Peer Topology & Source Bytes", "Topologia de Peers e Fontes")).strong());
            ui.add_space(6.0);
            if self.peer_bytes.is_empty() {
                ui.label(egui::RichText::new(tr(self.language, "No data transferred yet.", "Nenhum dado transferido ainda.")).weak());
            } else {
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    let mut sorted_peers: Vec<_> = self.peer_bytes.iter().collect();
                    sorted_peers.sort_by(|a, b| b.1.cmp(a.1));
                    for (source, bytes) in sorted_peers {
                        ui.horizontal(|ui| {
                            let source_color = if source.to_ascii_lowercase().contains("peer") {
                                egui::Color32::LIGHT_GREEN
                            } else {
                                egui::Color32::LIGHT_BLUE
                            };
                            ui.label(egui::RichText::new(format!("[{}]", source)).color(source_color));
                            let mb = **bytes as f64 / (1024.0 * 1024.0);
                            ui.label(format!("{:.2} MB transferred", mb));
                        });
                    }
                });
            }
        });

        ui.add_space(10.0);
        ui.label(egui::RichText::new(tr(self.language, "Recent Fragment Transfer Log", "Log Recente de Transferência de Fragmentos")).strong());
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .max_height(240.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.fragment_logs.is_empty() {
                    ui.label(egui::RichText::new(tr(self.language, "No fragment events recorded yet.", "Nenhum evento de fragmento registrado ainda.")).weak());
                } else {
                    for entry in self.fragment_logs.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&entry.timestamp).small().weak());
                            let source_color = if entry.is_peer {
                                egui::Color32::LIGHT_GREEN
                            } else {
                                egui::Color32::LIGHT_BLUE
                            };
                            ui.label(egui::RichText::new(format!("[{}]", entry.source)).small().color(source_color));
                            ui.label(egui::RichText::new(format!("Frag #{} of {}", entry.fragment_index, entry.file)).small());
                            ui.label(egui::RichText::new(format!("({}/{} bytes)", entry.downloaded_bytes, entry.total_bytes)).small().weak());
                        });
                    }
                }
            });
    }

    fn render_schedule_loop_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.heading(tr(self.language, "Synchronized Scheduling & Loop Testing", "Agendamento Sincronizado e Loop de Testes"));
        ui.add_space(8.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Synchronized Clock Trigger (Multi-Machine Sync)", "Gatilho Sincronizado (Multi-Máquinas)")).strong());
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr(self.language, "Set an exact local time to trigger simultaneous downloads on all machines:", "Defina um horário local exato para acionar downloads simultâneos em todas as máquinas:")).small().weak());
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(tr(self.language, "Trigger Time (HH:MM:SS):", "Horário de Acionamento (HH:MM:SS):"));
                ui.add(egui::TextEdit::singleline(&mut self.schedule_time_str).desired_width(100.0));

                let current_time_str = Local::now().format("%H:%M:%S").to_string();
                ui.label(format!("(Current: {})", current_time_str));

                if self.state == LauncherState::ScheduledWaiting {
                    if ui.button(egui::RichText::new("Cancel Schedule").color(egui::Color32::YELLOW)).clicked() {
                        self.schedule_active = false;
                        self.state = LauncherState::Idle;
                        self.status_text = "Schedule cancelled.".to_string();
                    }
                } else if ui.button(tr(self.language, "Arm Synchronized Schedule", "Armar Agendamento Sincronizado")).clicked() {
                    match NaiveTime::parse_from_str(&self.schedule_time_str, "%H:%M:%S") {
                        Ok(_) => {
                            self.schedule_active = true;
                            self.state = LauncherState::ScheduledWaiting;
                            self.status_text = format!("Armed. Waiting for clock to hit {}...", self.schedule_time_str);
                        }
                        Err(_) => {
                            self.error_message = Some("Invalid time format. Use HH:MM:SS (e.g. 15:30:00)".to_string());
                        }
                    }
                }
            });
        });

        ui.add_space(12.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(tr(self.language, "Automated Loop Testing (Continuous Stress / Benchmark)", "Testes em Loop Automatizados (Estresse Contínuo)")).strong());
            ui.add_space(6.0);

            ui.checkbox(&mut self.loop_enabled, tr(self.language, "Enable Multi-Cycle Loop Testing", "Habilitar Testes em Loop Multi-Ciclos"));
            if self.loop_enabled {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(tr(self.language, "Total Iterations:", "Iterações Totais:"));
                    ui.add(egui::DragValue::new(&mut self.loop_total_cycles).range(1..=100));
                    ui.separator();
                    ui.label(tr(self.language, "Cooldown Between Cycles (s):", "Intervalo Entre Ciclos (s):"));
                    ui.add(egui::DragValue::new(&mut self.loop_cooldown_seconds).range(1..=60));
                });
                ui.checkbox(&mut self.loop_auto_clean, tr(self.language, "Auto-clean cache and files before each cycle", "Limpar cache e arquivos automaticamente antes de cada ciclo"));
            }
        });

        ui.add_space(12.0);

        if !self.reports_history.is_empty() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Completed Cycles ({})", self.reports_history.len())).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr(self.language, "Clear History", "Limpar Histórico")).clicked() {
                            self.reports_history.clear();
                        }
                    });
                });

                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for (cycle, timestamp, report) in &self.reports_history {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Cycle #{}", cycle)).strong());
                            ui.label(format!("Time: {}", timestamp));
                            ui.label(format!("Duration: {} ms", report.elapsed_ms));
                            ui.label(egui::RichText::new(format!("P2P: {:.1}%", report.p2p_percent())).color(egui::Color32::LIGHT_GREEN));
                            ui.label(egui::RichText::new(format!("Offload: {:.1}%", report.offload_percent())).color(egui::Color32::LIGHT_BLUE));
                        });
                    }
                });
            });
        }
    }

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.heading("Settings, Endpoints & Directories");
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Server Endpoints & Authentication").strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Origin URL (HTTP/HTTPS):");
                    ui.add(egui::TextEdit::singleline(&mut self.input_origin_url).desired_width(340.0));
                    if ui.button("Test Connection").clicked() {
                        self.test_connection();
                    }
                });

                if let Some((success, msg)) = &self.connection_test_feedback {
                    let color = if *success { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED };
                    ui.label(egui::RichText::new(msg).small().color(color));
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Application Token:");
                    ui.add(egui::TextEdit::singleline(&mut self.input_token).password(true).desired_width(340.0));
                });
            });

            ui.add_space(10.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Target Release & Bucket").strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(tr(self.language, "Release Bucket:", "Bucket do Release:"));
                    ui.add(egui::TextEdit::singleline(&mut self.input_bucket).desired_width(220.0));
                });
                ui.horizontal(|ui| {
                    ui.label(tr(self.language, "Manifest Key:", "Chave do Manifesto:"));
                    ui.add(egui::TextEdit::singleline(&mut self.input_manifest).desired_width(220.0));
                });
            });

            ui.add_space(10.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Local Directories & Storage").strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Install Directory:");
                    ui.add(egui::TextEdit::singleline(&mut self.input_install_dir).desired_width(360.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Cache Directory:");
                    ui.add(egui::TextEdit::singleline(&mut self.input_cache_dir).desired_width(360.0));
                });
            });

            ui.add_space(10.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Peer-to-Peer (Libp2p) Networking").strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("P2P Listen Address:");
                    ui.add(egui::TextEdit::singleline(&mut self.input_p2p_listen).desired_width(280.0));
                });
                ui.horizontal(|ui| {
                    ui.label("P2P Announce Address (Optional):");
                    ui.add(egui::TextEdit::singleline(&mut self.input_p2p_announce).desired_width(280.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Post-Download Seeding (seconds):");
                    ui.add(egui::TextEdit::singleline(&mut self.input_seed_seconds).desired_width(80.0));
                });
            });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("Save Configuration").strong()).clicked() {
                    match self.apply_inputs_to_config() {
                        Ok(()) => {
                            match self.config.save(&self.config_path) {
                                Ok(()) => self.settings_feedback = Some((true, format!("Saved to {}", self.config_path.display()))),
                                Err(e) => self.settings_feedback = Some((false, e)),
                            }
                        }
                        Err(e) => self.settings_feedback = Some((false, e)),
                    }
                }

                if ui.button("Reload from .env / toml").clicked() {
                    self.config = LauncherConfig::load(&self.config_path).unwrap_or_default();
                    self.sync_inputs_from_config();
                    self.settings_feedback = Some((true, "Configuration reloaded from disk / .env".to_string()));
                }
            });

            if let Some((success, msg)) = &self.settings_feedback {
                ui.add_space(4.0);
                let color = if *success { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED };
                ui.label(egui::RichText::new(msg).color(color));
            }
        });
    }
}
