#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod launcher;

use std::{env, path::PathBuf, thread};
use eframe::egui;
use pontemesh_sdk_core::CancellationToken;

use config::LauncherConfig;
use launcher::{GameLauncher, ProgressMessage, InstallReport};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Ponte Mesh Launcher UI",
        options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
}

struct LauncherApp {
    config: LauncherConfig,
    state: LauncherState,
    cancellation: Option<CancellationToken>,
    rx: Option<std::sync::mpsc::Receiver<ProgressMessage>>,
    status_text: String,
    progress_percent: f32,
    progress_details: String,
    report: Option<InstallReport>,
    error: Option<String>,
}

#[derive(PartialEq)]
enum LauncherState {
    Idle,
    Downloading,
    Ready,
    Error,
}

impl LauncherApp {
    fn new() -> Self {
        let config_path = env::var_os("PONTEMESH_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("launcher.toml"));
        
        let config = LauncherConfig::load(&config_path).unwrap_or_else(|e| {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        });

        let mut app = Self {
            config,
            state: LauncherState::Idle,
            cancellation: None,
            rx: None,
            status_text: "Ready to check for updates.".to_string(),
            progress_percent: 0.0,
            progress_details: String::new(),
            report: None,
            error: None,
        };
        app.start_download();
        app
    }

    fn start_download(&mut self) {
        self.state = LauncherState::Downloading;
        self.status_text = "Starting update...".to_string();
        self.progress_percent = 0.0;
        self.progress_details = String::new();
        self.error = None;
        self.report = None;

        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        let cancellation = CancellationToken::default();
        self.cancellation = Some(cancellation.clone());

        let config = self.config.clone();
        
        thread::spawn(move || {
            let launcher = GameLauncher::new(config);
            match launcher.install_latest(cancellation, tx.clone()) {
                Ok(_) => { /* Done is sent inside install_latest */ }
                Err(e) => {
                    let _ = tx.send(ProgressMessage::Error(e));
                }
            }
        });
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ProgressMessage::Status(status) => {
                        self.status_text = status;
                    }
                    ProgressMessage::Progress { file, downloaded, total, percent } => {
                        self.progress_percent = percent as f32 / 100.0;
                        self.progress_details = format!("{} ({}/{} bytes)", file, downloaded, total);
                    }
                    ProgressMessage::Done(report) => {
                        self.report = Some(report.clone());
                        self.state = LauncherState::Ready;
                        self.status_text = "Game is ready to play!".to_string();
                        report.print();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ProgressMessage::Error(err) => {
                        self.error = Some(err);
                        self.state = LauncherState::Error;
                    }
                }
            }
            // Ask egui to repaint while downloading so we see progress
            if self.state == LauncherState::Downloading {
                ctx.request_repaint();
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Ponte Mesh Game Launcher");
            ui.add_space(20.0);

            ui.label(format!("Origin: {}", self.config.origin_url));
            ui.label(format!("Release: {}/{}", self.config.release_bucket, self.config.release_manifest_key));
            ui.add_space(20.0);

            ui.separator();
            ui.add_space(20.0);

            match self.state {
                LauncherState::Idle => {
                    if ui.button("Check & Update").clicked() {
                        self.start_download();
                    }
                }
                LauncherState::Downloading => {
                    ui.label(&self.status_text);
                    ui.add(egui::ProgressBar::new(self.progress_percent).show_percentage());
                    ui.label(&self.progress_details);
                    
                    ui.add_space(10.0);
                    if ui.button("Cancel").clicked() {
                        if let Some(cancel) = &self.cancellation {
                            cancel.cancel();
                            self.state = LauncherState::Idle;
                            self.status_text = "Update cancelled.".to_string();
                        }
                    }
                }
                LauncherState::Ready => {
                    ui.label(egui::RichText::new(&self.status_text).color(egui::Color32::GREEN));
                    if let Some(report) = &self.report {
                        ui.add_space(10.0);
                        ui.label(format!("Version: {}", report.version));
                        ui.label(format!("Time: {} ms", report.elapsed_ms));
                        ui.label(format!("Size: {} bytes", report.bytes));
                    }
                    ui.add_space(20.0);
                    if ui.button("Play Game").clicked() {
                        // Normally this would launch the game binary
                        self.status_text = "Launching game... (not implemented in demo)".to_string();
                    }
                    ui.add_space(10.0);
                    if ui.button("Verify Files Again").clicked() {
                        self.start_download();
                    }
                }
                LauncherState::Error => {
                    ui.label(egui::RichText::new("Error!").color(egui::Color32::RED));
                    if let Some(err) = &self.error {
                        ui.label(err);
                    }
                    ui.add_space(20.0);
                    if ui.button("Retry").clicked() {
                        self.start_download();
                    }
                }
            }
        });
    }
}
