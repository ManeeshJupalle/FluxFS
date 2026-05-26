//! egui settings application.

use crate::config::{
    config_file_path, ensure_data_dir, load_config, save_user_config, watch_rulesets_from_config,
    FluxConfig, WatchConfig, WatchRule,
};
use crate::dedup::build_report;
use crate::errors::FluxError;
use crate::index::{index_file_path, load};
use crate::ipc::is_paused;
use crate::reporting::activity::{activity_log_path, format_entry_plain, read_entries, LogFilter, WeeklySummary};
use crate::reporting::activity::weekly_summary;
use crate::reporting::format::{format_bytes, format_last_scan, format_uptime, home_dir};
use crate::rules::{organize_index, OrganizeSummary};
use crate::service::{service_kind_label, service_status};
use crate::watcher::daemon::{daemon_started_path, is_daemon_running, pid_file_path, read_daemon_started, read_pid_file};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Launch the FluxFS settings window (blocks until closed).
pub fn run_settings_app() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FluxFS Settings",
        native_options,
        Box::new(|cc| Ok(Box::new(SettingsApp::new(cc)?))),
    )
    .map_err(|err| anyhow::anyhow!("Settings window failed: {err}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Status,
    WatchRules,
    Dedup,
    Activity,
}

pub struct StatusSnapshot {
    pub daemon_running: bool,
    pub daemon_paused: bool,
    pub pid: Option<u32>,
    pub uptime: String,
    pub service_label: String,
    pub file_count: usize,
    pub total_size: String,
    pub last_scan: String,
    pub duplicate_groups: usize,
    pub weekly: WeeklySummary,
}

struct SettingsApp {
    config: FluxConfig,
    config_path: PathBuf,
    tab: Tab,
    selected_watch: usize,
    status: Option<StatusSnapshot>,
    activity_lines: Vec<String>,
    status_message: String,
    error_message: String,
    dry_run_summary: String,
}

impl SettingsApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let config = load_config()?;
        let config_path = config_file_path()?;
        let mut app = Self {
            config,
            config_path,
            tab: Tab::Status,
            selected_watch: 0,
            status: None,
            activity_lines: Vec::new(),
            status_message: String::new(),
            error_message: String::new(),
            dry_run_summary: String::new(),
        };
        app.refresh_dashboard();
        Ok(app)
    }

    fn refresh_dashboard(&mut self) {
        self.error_message.clear();
        match load_dashboard(&self.config) {
            Ok((status, activity)) => {
                self.status = Some(status);
                self.activity_lines = activity;
            }
            Err(err) => self.error_message = err.to_string(),
        }
    }

    fn reload_config(&mut self) {
        match load_config() {
            Ok(cfg) => {
                self.config = cfg;
                self.config_path = config_file_path().unwrap_or_else(|_| self.config_path.clone());
                self.selected_watch = self.selected_watch.min(self.config.watch.len().saturating_sub(1));
                self.status_message = "Reloaded config from disk.".to_string();
                self.refresh_dashboard();
            }
            Err(err) => self.error_message = err.to_string(),
        }
    }

    fn save_config(&mut self) {
        match save_user_config(&self.config) {
            Ok(path) => {
                self.config_path = path;
                self.status_message = format!("Saved to {}", self.config_path.display());
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = err.to_string();
                self.status_message.clear();
            }
        }
    }

    fn run_dry_run(&mut self) {
        self.dry_run_summary.clear();
        self.error_message.clear();
        match run_organize_dry_run(&self.config) {
            Ok(summary) => {
                self.dry_run_summary = format!(
                    "Dry-run: {} file(s) would move, {} skipped.",
                    summary.dry_run, summary.skipped
                );
            }
            Err(err) => self.error_message = err.to_string(),
        }
    }

    fn pick_watch_folder(&mut self) {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let path = path_to_config_string(&folder);
        if !self.config.watch.iter().any(|w| w.path == path) {
            self.config.watch.push(WatchConfig {
                path,
                rules: Vec::new(),
            });
            self.selected_watch = self.config.watch.len() - 1;
        }
    }

    fn pick_rule_destination(&mut self, watch_idx: usize, rule_idx: usize) {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        if let Some(rule) = self.config.watch.get_mut(watch_idx).and_then(|w| w.rules.get_mut(rule_idx)) {
            rule.destination = format!("{}/", path_to_config_string(&folder).trim_end_matches('/'));
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("FluxFS Settings");
                ui.separator();
                if ui.button("💾 Save").clicked() {
                    self.save_config();
                }
                if ui.button("↻ Reload").clicked() {
                    self.reload_config();
                }
                if ui.button("🔄 Refresh status").clicked() {
                    self.refresh_dashboard();
                }
            });
            if !self.status_message.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(60, 160, 90), &self.status_message);
            }
            if !self.error_message.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(220, 70, 70), &self.error_message);
            }
            ui.label(format!("Config: {}", self.config_path.display()));
        });

        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Test rules (dry-run organize)").clicked() {
                    self.run_dry_run();
                }
                if !self.dry_run_summary.is_empty() {
                    ui.label(&self.dry_run_summary);
                }
            });
        });

        egui::SidePanel::left("tabs").resizable(false).default_width(140.0).show(ctx, |ui| {
            ui.selectable_value(&mut self.tab, Tab::Status, "📊 Status");
            ui.selectable_value(&mut self.tab, Tab::WatchRules, "📁 Watch & rules");
            ui.selectable_value(&mut self.tab, Tab::Dedup, "🔍 Dedup");
            ui.selectable_value(&mut self.tab, Tab::Activity, "📜 Activity");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::Status => self.show_status(ui),
                Tab::WatchRules => self.show_watch_rules(ui),
                Tab::Dedup => self.show_dedup(ui),
                Tab::Activity => self.show_activity(ui),
            }
        });
    }
}

impl SettingsApp {
    fn show_status(&self, ui: &mut egui::Ui) {
        ui.heading("System status");
        let Some(status) = &self.status else {
            ui.label("Status unavailable.");
            return;
        };

        egui::Grid::new("status_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Daemon:");
            if status.daemon_running {
                let paused = if status.daemon_paused { " (paused)" } else { "" };
                ui.colored_label(
                    egui::Color32::from_rgb(60, 160, 90),
                    format!("Running{paused}"),
                );
            } else {
                ui.colored_label(egui::Color32::from_rgb(220, 70, 70), "Stopped");
            }
            ui.end_row();

            if let Some(pid) = status.pid {
                ui.label("PID / uptime:");
                ui.label(format!("{pid} — {}", status.uptime));
                ui.end_row();
            }

            ui.label("Auto-start:");
            ui.label(&status.service_label);
            ui.end_row();

            ui.label("Indexed files:");
            ui.label(format!("{} ({})", status.file_count, status.total_size));
            ui.end_row();

            ui.label("Last scan:");
            ui.label(&status.last_scan);
            ui.end_row();

            ui.label("Duplicate groups:");
            ui.label(status.duplicate_groups.to_string());
            ui.end_row();
        });

        ui.add_space(12.0);
        ui.heading("This week");
        ui.label(format!("Files organized: {}", status.weekly.files_organized));
        ui.label(format!("Duplicates caught: {}", status.weekly.duplicates_caught));
        ui.label(format!(
            "Space saved: {}",
            format_bytes(status.weekly.space_saved)
        ));
    }

    fn show_watch_rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Watch folders");
        ui.horizontal(|ui| {
            if ui.button("+ Add folder").clicked() {
                self.pick_watch_folder();
            }
        });

        if self.config.watch.is_empty() {
            ui.label("No watch folders — add at least one.");
            return;
        }

        self.selected_watch = self.selected_watch.min(self.config.watch.len() - 1);

        ui.horizontal(|ui| {
            for (idx, watch) in self.config.watch.iter().enumerate() {
                ui.selectable_value(&mut self.selected_watch, idx, &watch.path);
            }
        });

        if ui.button("Remove selected folder").clicked() {
            if self.config.watch.len() > 1 {
                self.config.watch.remove(self.selected_watch);
                self.selected_watch = self.selected_watch.min(self.config.watch.len() - 1);
            } else {
                self.error_message = "At least one watch folder is required.".to_string();
            }
        }

        let watch_idx = self.selected_watch;
        ui.separator();
        ui.heading("Rules");
        ui.label("First matching rule wins. Patterns: *.pdf, contains:text, older:90d");

        let mut remove_rule: Option<usize> = None;
        let mut pick_dest_rule: Option<usize> = None;
        if let Some(watch) = self.config.watch.get_mut(watch_idx) {
            egui::Grid::new("rules_grid").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Pattern");
                ui.label("Destination");
                ui.label("");
                ui.end_row();

                for (rule_idx, rule) in watch.rules.iter_mut().enumerate() {
                    ui.text_edit_singleline(&mut rule.pattern);
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut rule.destination);
                        if ui.small_button("📂").clicked() {
                            pick_dest_rule = Some(rule_idx);
                        }
                    });
                    if ui.button("✕").clicked() {
                        remove_rule = Some(rule_idx);
                    }
                    ui.end_row();
                }
            });

            if ui.button("+ Add rule").clicked() {
                watch.rules.push(WatchRule {
                    pattern: "*.pdf".to_string(),
                    destination: "~/Documents/".to_string(),
                });
            }

            if let Some(idx) = remove_rule {
                watch.rules.remove(idx);
            }
        }

        if let Some(rule_idx) = pick_dest_rule {
            self.pick_rule_destination(watch_idx, rule_idx);
        }
    }

    fn show_dedup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Duplicate handling");
        egui::Grid::new("dedup_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Strategy:");
            egui::ComboBox::from_id_salt("dedup_strategy")
                .selected_text(&self.config.duplicates.strategy)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.config.duplicates.strategy, "report".to_string(), "report");
                    ui.selectable_value(&mut self.config.duplicates.strategy, "trash".to_string(), "trash");
                    ui.selectable_value(&mut self.config.duplicates.strategy, "delete".to_string(), "delete");
                });
            ui.end_row();

            ui.label("Min size:");
            ui.text_edit_singleline(&mut self.config.duplicates.min_size);
            ui.end_row();

            ui.label("Max hash size:");
            ui.text_edit_singleline(&mut self.config.duplicates.max_hash_size);
            ui.end_row();

            ui.label("Global dry-run:");
            ui.checkbox(&mut self.config.general.dry_run, "Preview moves without changing files");
            ui.end_row();
        });
    }

    fn show_activity(&self, ui: &mut egui::Ui) {
        ui.heading("Recent activity");
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.activity_lines.is_empty() {
                ui.label("No activity yet.");
            } else {
                for line in &self.activity_lines {
                    ui.label(line);
                }
            }
        });
    }
}

fn load_dashboard(config: &FluxConfig) -> Result<(StatusSnapshot, Vec<String>), FluxError> {
    let data_dir = ensure_data_dir(config)?;
    let activity_log = activity_log_path(&data_dir);
    let index_path = index_file_path(config)?;
    let index = load(&index_path)?;
    let stats = index.stats();
    let dup_report = build_report(&index);
    let weekly = weekly_summary(&activity_log)?;

    let running = is_daemon_running(&data_dir)?;
    let paused = is_paused(&data_dir);
    let pid = if running {
        read_pid_file(&pid_file_path(&data_dir)).ok()
    } else {
        None
    };
    let uptime = if running {
        daemon_started_path(&data_dir)
            .exists()
            .then(|| read_daemon_started(&daemon_started_path(&data_dir)).ok())
            .flatten()
            .map(|started| {
                let elapsed = chrono::Utc::now().signed_duration_since(started);
                format_uptime(Duration::from_secs(elapsed.num_seconds().max(0) as u64))
            })
            .unwrap_or_else(|| "—".to_string())
    } else {
        "—".to_string()
    };

    let service = service_status(&data_dir)?;
    let service_label = if service.installed {
        service
            .kind
            .map(service_kind_label)
            .unwrap_or("registered")
            .to_string()
    } else {
        "Not installed".to_string()
    };

    let status = StatusSnapshot {
        daemon_running: running,
        daemon_paused: paused,
        pid,
        uptime,
        service_label,
        file_count: stats.total_files,
        total_size: format_bytes(stats.total_size),
        last_scan: format_last_scan(stats.last_scan),
        duplicate_groups: dup_report.groups.len(),
        weekly,
    };

    let entries = read_entries(
        &activity_log,
        &LogFilter {
            limit: Some(50),
            today_only: false,
        },
    )?;
    let home = home_dir();
    let activity_lines: Vec<String> = entries
        .iter()
        .map(|e| format_entry_plain(e, home.as_deref()))
        .collect();

    Ok((status, activity_lines))
}

fn run_organize_dry_run(config: &FluxConfig) -> Result<OrganizeSummary, FluxError> {
    let data_dir = ensure_data_dir(config)?;
    let index_path = index_file_path(config)?;
    let mut index = load(&index_path)?;
    if index.is_empty() {
        return Err(FluxError::Index(
            "Index is empty. Run `flux init` or `flux setup` first.".to_string(),
        ));
    }
    let watch_rulesets = watch_rulesets_from_config(config)?;
    let activity_log = activity_log_path(&data_dir);
    organize_index(&mut index, &watch_rulesets, true, &activity_log)
}

fn path_to_config_string(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if path.starts_with(&home) {
            let rest = path.strip_prefix(&home).unwrap_or(path);
            let suffix = rest.to_string_lossy().replace('\\', "/");
            if suffix.is_empty() {
                return "~".to_string();
            }
            return format!("~/{suffix}");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}
