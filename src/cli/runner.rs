//! CLI command dispatch for the `flux` / `fluxfs` binaries.

use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use std::fs::OpenOptions;
use std::path::Path;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use crate::cli::commands::{Cli, Commands};
use crate::config::{
    config_file_path, ensure_data_dir, load_config, load_config_from_path, save_default_config,
    watch_rulesets_from_config, FluxConfig,
};
use crate::dedup::{build_report, resolve_duplicates, DuplicateReport};
use crate::errors::FluxError;
use crate::hasher::hash_all;
use crate::index::{
    display::print_find_results, index_file_path, load, save, search, FileIndex, SearchOptions,
    SortMode,
};
use crate::reporting::activity::{
    activity_log_path, log_scan_completed, print_activity_log, read_entries, LogFilter,
    DEFAULT_LOG_LIMIT,
};
use crate::reporting::status::print_status;
use crate::rules::{organize_index, OrganizeSummary};
use crate::scanner::scan_directories;
use crate::service;
use crate::watcher::daemon::daemon_log_path;
use crate::watcher::{is_daemon_running, run_daemon, stop_daemon};

/// Run the FluxFS CLI (used by `flux` and `fluxfs` binaries).
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => run_init()?,
        Commands::Config => run_config()?,
        Commands::Dedup { dry_run, confirm } => run_dedup(*dry_run, *confirm)?,
        Commands::Organize { dry_run } => run_organize(*dry_run)?,
        Commands::Start {
            foreground,
            daemon,
        } => run_start(*foreground, *daemon)?,
        Commands::Stop => run_stop()?,
        Commands::InstallService => run_install_service(false)?,
        Commands::UninstallService => run_uninstall_service()?,
        Commands::Setup {
            skip_init,
            skip_service,
            quiet,
        } => run_setup(*skip_init, *skip_service, *quiet)?,
        Commands::Settings => run_settings()?,
        Commands::Find {
            query,
            path,
            exact,
            ext,
            sort,
        } => run_find(query, *path, *exact, ext.as_deref(), (*sort).into())?,
        Commands::Status => run_status()?,
        Commands::Log { all, today, count } => run_log(*all, *today, *count)?,
    }

    Ok(())
}

/// Where tracing output should go.
enum LogDestination<'a> {
    Stderr,
    File(&'a Path),
}

/// Initialize tracing subscriber from config log level (overridable via `RUST_LOG`).
fn init_logging(config: &FluxConfig, dest: LogDestination<'_>) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.general.log_level.as_str()))
        .context("Invalid log level in config or RUST_LOG environment variable")?;

    match dest {
        LogDestination::Stderr => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?;
        }
        LogDestination::File(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).context("Failed to create log directory")?;
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("Failed to open log file {}", path.display()))?;
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .with_writer(file)
                .with_ansi(false)
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?;
        }
    }

    Ok(())
}

/// Log config path and watched directories at debug level on startup.
fn log_startup(config: &FluxConfig) -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    debug!(path = %config_path.display(), "Config file location");

    let watch_paths = config.watch_paths()?;
    for path in &watch_paths {
        debug!(watch = %path.display(), "Watching directory");
    }

    Ok(())
}

/// `flux init` — create config/data dirs, scan watch paths, hash, build and save index.
fn run_init() -> anyhow::Result<()> {
    let config_path = config_file_path()?;

    if config_path.exists() {
        let cfg = load_config_from_path(&config_path)?;
        init_logging(&cfg, LogDestination::Stderr)?;
        info!(path = %config_path.display(), "Config already exists");
    } else {
        let path = save_default_config()?;
        let cfg = load_config_from_path(&path)?;
        init_logging(&cfg, LogDestination::Stderr)?;
        info!(path = %path.display(), "Created default config");
    }

    let cfg = load_config()?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    info!(path = %data_dir.display(), "Data directory ready");

    let watch_paths = cfg.watch_paths()?;
    info!(directories = watch_paths.len(), "Starting filesystem scan");

    let (entries, summary) = scan_directories(&watch_paths, &cfg.index)?;
    let mut index = FileIndex::from_entries(entries, summary.duration_ms);

    let hash_stats = hash_all(&mut index, &cfg.duplicates)?;
    info!(
        hashed = hash_stats.hashed,
        skipped = hash_stats.skipped,
        failed = hash_stats.failed,
        "Content hashing complete"
    );

    let dup_report = build_report(&index);
    print_duplicate_report(&dup_report, "Duplicates after init");

    let index_path = index_file_path(&cfg)?;
    save(&index, &index_path)?;

    let activity_log = activity_log_path(&data_dir);
    log_scan_completed(&activity_log, index.len(), summary.duration_ms)?;

    let loaded = load(&index_path)?;
    debug!(
        path = %index_path.display(),
        files = loaded.len(),
        "Index verified after save"
    );
    info!(
        path = %index_path.display(),
        files = index.len(),
        total_size = index.stats().total_size,
        "Index saved"
    );

    println!("FluxFS initialized.");
    println!("  Config:      {}", config_path.display());
    println!("  Data:        {}", data_dir.display());
    println!("  Index:       {}", index_path.display());
    println!();
    println!("  Scan summary");
    println!("  ────────────────────────────────────");
    println!("  Directories: {}", summary.directories_scanned);
    println!("  Files:       {}", summary.file_count);
    println!("  Total size:  {}", format_bytes(summary.total_size_bytes));
    println!("  Duration:    {:.2}s", summary.duration_ms as f64 / 1000.0);
    println!();
    println!("  Hashing");
    println!("  ────────────────────────────────────");
    println!("  Hashed:      {}", hash_stats.hashed);
    println!("  Skipped:     {}", hash_stats.skipped);
    if hash_stats.failed > 0 {
        println!("  Failed:      {}", hash_stats.failed);
    }

    Ok(())
}

/// `flux dedup` — hash unhashed files, report duplicates, apply strategy.
fn run_dedup(cli_dry_run: bool, confirm_delete: bool) -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    let index_path = index_file_path(&cfg)?;
    let mut index = load(&index_path)?;

    if index.is_empty() {
        return Err(FluxError::Index(
            "Index is empty. Run `flux init` first to scan and build the index.".to_string(),
        )
        .into());
    }

    let hash_stats = hash_all(&mut index, &cfg.duplicates)?;
    info!(
        hashed = hash_stats.hashed,
        skipped = hash_stats.skipped,
        "Hashing complete for dedup"
    );

    // Persist any newly computed (or refreshed) hashes immediately so that a
    // dry-run or report-only invocation does not throw away expensive work.
    // Without this, the next `flux dedup` would re-hash the same files.
    if hash_stats.hashed > 0 {
        save(&index, &index_path)?;
    }

    let report = build_report(&index);
    print_duplicate_report(&report, "Duplicate scan");

    let dry_run = cfg.general.dry_run || cli_dry_run;
    let trash_dir = data_dir.join("trash");
    let activity_log = activity_log_path(&data_dir);

    let resolve_summary = resolve_duplicates(
        &mut index,
        &cfg.duplicates,
        &trash_dir,
        &activity_log,
        dry_run,
        confirm_delete,
    )?;

    if dry_run {
        println!();
        println!(
            "{}",
            "Dry-run mode: no files were moved or deleted.".yellow()
        );
    } else if cfg.duplicates.strategy == "report" {
        println!();
        println!(
            "{}",
            "Report-only strategy: no files were modified.".bright_black()
        );
    } else {
        save(&index, &index_path)?;
        println!();
        println!("  Resolution");
        println!("  ────────────────────────────────────");
        println!("  Strategy:    {}", cfg.duplicates.strategy);
        println!("  Removed:     {}", resolve_summary.files_removed);
        println!(
            "  Reclaimed:   {}",
            format_bytes(resolve_summary.bytes_reclaimed)
        );
    }

    Ok(())
}

/// `flux organize` — match files to rules and move/copy (first match wins).
fn run_organize(cli_dry_run: bool) -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    let index_path = index_file_path(&cfg)?;
    let mut index = load(&index_path)?;

    if index.is_empty() {
        return Err(FluxError::Index(
            "Index is empty. Run `flux init` first to scan and build the index.".to_string(),
        )
        .into());
    }

    let watch_rulesets = watch_rulesets_from_config(&cfg)?;
    let dry_run = cfg.general.dry_run || cli_dry_run;
    let activity_log = activity_log_path(&data_dir);

    let summary = organize_index(&mut index, &watch_rulesets, dry_run, &activity_log)?;

    print_organize_summary(&summary, dry_run);

    if !dry_run && summary.organized > 0 {
        save(&index, &index_path)?;
        info!(path = %index_path.display(), "Index saved after organize");
    }

    Ok(())
}

/// `flux status` — daemon state, index stats, and attention items.
fn run_status() -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    let index_path = index_file_path(&cfg)?;
    let index = load(&index_path)?;
    let activity_log = activity_log_path(&data_dir);

    print_status(&cfg, &data_dir, &index, &activity_log)?;

    Ok(())
}

/// `flux log` — show recent activity entries.
fn run_log(all: bool, today: bool, count: Option<usize>) -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;

    let data_dir = ensure_data_dir(&cfg)?;
    let activity_log = activity_log_path(&data_dir);

    let limit = if all {
        None
    } else {
        Some(count.unwrap_or(DEFAULT_LOG_LIMIT))
    };

    let filter = LogFilter {
        limit,
        today_only: today,
    };

    let entries = read_entries(&activity_log, &filter)?;
    print_activity_log(&entries, !all);

    Ok(())
}

/// `flux find` — fuzzy or glob search over the index.
fn run_find(
    query: &str,
    match_path: bool,
    exact_glob: bool,
    extension: Option<&str>,
    sort: SortMode,
) -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    let index_path = index_file_path(&cfg)?;
    let index = load(&index_path)?;

    if index.is_empty() {
        return Err(FluxError::Index(
            "Index is empty. Run `flux init` first to scan and build the index.".to_string(),
        )
        .into());
    }

    let options = SearchOptions {
        match_path,
        exact_glob,
        extension: extension.map(str::to_string),
        sort,
    };

    let output = search(&index, query, cfg.search.max_results, &options);
    print_find_results(&output);

    Ok(())
}

/// `flux start` — run or spawn the file watcher daemon.
fn run_start(foreground: bool, daemon: bool) -> anyhow::Result<()> {
    let cfg = load_config()?;
    let data_dir = ensure_data_dir(&cfg)?;

    if daemon {
        init_logging(
            &cfg,
            LogDestination::File(&daemon_log_path(&data_dir)),
        )?;
        log_startup(&cfg)?;

        if is_daemon_running(&data_dir)? {
            return Err(FluxError::Watcher(
                "FluxFS daemon is already running. Use `flux stop` first.".to_string(),
            )
            .into());
        }

        let runtime =
            tokio::runtime::Runtime::new().context("Failed to start tokio runtime for daemon")?;
        runtime.block_on(run_daemon(cfg))?;
        return Ok(());
    }

    if foreground {
        init_logging(&cfg, LogDestination::Stderr)?;
        log_startup(&cfg)?;

        if is_daemon_running(&data_dir)? {
            return Err(FluxError::Watcher(
                "FluxFS daemon is already running. Use `flux stop` first.".to_string(),
            )
            .into());
        }

        println!("FluxFS daemon running (foreground). Press Ctrl+C to stop.");
        println!("  Data: {}", data_dir.display());

        let runtime =
            tokio::runtime::Runtime::new().context("Failed to start tokio runtime for daemon")?;
        runtime.block_on(run_daemon(cfg))?;
        return Ok(());
    }

    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    if is_daemon_running(&data_dir)? {
        return Err(FluxError::Watcher(
            "FluxFS daemon is already running. Use `flux stop` first.".to_string(),
        )
        .into());
    }

    let binary = std::env::current_exe().context("Failed to resolve flux binary path")?;

    if service::is_service_installed(&data_dir) {
        service::start_service(&binary)?;
        println!("FluxFS service started.");
    } else {
        service::spawn_detached_daemon(&binary)?;
        println!("FluxFS daemon started in the background.");
        println!("  Logs:  {}", daemon_log_path(&data_dir).display());
        println!("  Stop:  flux stop");
        println!("  Boot:  flux install-service");
    }

    Ok(())
}

/// `flux install-service` — register FluxFS to start at login.
fn run_install_service(quiet: bool) -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    let binary = std::env::current_exe().context("Failed to resolve flux binary path")?;

    if service::is_service_installed(&data_dir) {
        return Err(FluxError::Watcher(
            "FluxFS service is already installed. Run `flux uninstall-service` first.".to_string(),
        )
        .into());
    }

    service::install_service(&binary, &data_dir)?;

    if let Ok(tray) = service::tray_binary_path() {
        if tray.exists() {
            service::spawn_tray(&tray)?;
        }
    }

    if !is_daemon_running(&data_dir)? {
        service::start_service(&binary)?;
    }

    let status = service::service_status(&data_dir)?;
    let kind = status
        .kind
        .map(service::service_kind_label)
        .unwrap_or("service");

    if quiet {
        return Ok(());
    }

    println!("FluxFS service installed and started.");
    println!("  Mode:   {kind}");
    println!("  Logs:   {}", daemon_log_path(&data_dir).display());
    println!("  Stop:   flux stop");
    println!("  Tray:   fluxfs-tray (system tray icon)");
    println!("  Remove: flux uninstall-service");

    Ok(())
}

/// `flux setup` — init + install-service (post-install hook for packaged installs).
fn run_setup(skip_init: bool, skip_service: bool, quiet: bool) -> anyhow::Result<()> {
    if !skip_init {
        run_init_inner(quiet)?;
    } else if !quiet {
        println!("Skipping init (--skip-init).");
    }

    if !skip_service {
        let cfg = load_config()?;
        if !quiet {
            init_logging(&cfg, LogDestination::Stderr)?;
        }
        let data_dir = ensure_data_dir(&cfg)?;

        if service::is_service_installed(&data_dir) {
            if !quiet {
                println!("Service already registered — ensuring daemon is running.");
            }
            if !is_daemon_running(&data_dir)? {
                let binary =
                    std::env::current_exe().context("Failed to resolve flux binary path")?;
                service::start_service(&binary)?;
            }
        } else {
            run_install_service(quiet)?;
        }
    } else if !quiet {
        println!("Skipping service install (--skip-service).");
    }

    if !quiet {
        println!();
        println!("FluxFS setup complete.");
        println!("  Tray:   fluxfs-tray");
        println!("  Status: flux status");
    }

    Ok(())
}

/// `flux init` with optional quiet output for installer hooks.
fn run_init_inner(quiet: bool) -> anyhow::Result<()> {
    if quiet {
        run_init_silent()
    } else {
        run_init()
    }
}

/// Init without user-facing banners (installer post-install).
fn run_init_silent() -> anyhow::Result<()> {
    let config_path = config_file_path()?;

    if !config_path.exists() {
        save_default_config()?;
    }

    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    let data_dir = ensure_data_dir(&cfg)?;
    let watch_paths = cfg.watch_paths()?;
    let (entries, summary) = scan_directories(&watch_paths, &cfg.index)?;
    let mut index = FileIndex::from_entries(entries, summary.duration_ms);
    let _ = hash_all(&mut index, &cfg.duplicates)?;
    let index_path = index_file_path(&cfg)?;
    save(&index, &index_path)?;
    let activity_log = activity_log_path(&data_dir);
    log_scan_completed(&activity_log, index.len(), summary.duration_ms)?;
    Ok(())
}

/// `flux uninstall-service` — remove login startup registration.
fn run_uninstall_service() -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;

    let data_dir = ensure_data_dir(&cfg)?;

    if !service::is_service_installed(&data_dir) {
        return Err(FluxError::Watcher(
            "FluxFS service is not installed. Run `flux install-service` first.".to_string(),
        )
        .into());
    }

    if is_daemon_running(&data_dir)? {
        stop_daemon(&data_dir)?;
        let _ = service::stop_service();
    }

    service::uninstall_service(&data_dir)?;

    println!("FluxFS service uninstalled.");
    println!("  Config and index were kept.");
    println!("  Start manually with: flux start");

    Ok(())
}

/// `flux settings` — open the settings GUI.
fn run_settings() -> anyhow::Result<()> {
    crate::gui::run_settings_app()
}

/// `flux stop` — stop the running daemon.
fn run_stop() -> anyhow::Result<()> {
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;

    let data_dir = ensure_data_dir(&cfg)?;

    if service::is_service_installed(&data_dir) {
        let _ = service::stop_service();
    }

    stop_daemon(&data_dir)?;

    println!("FluxFS daemon stopped.");

    Ok(())
}

/// `flux config` — load and pretty-print the current configuration.
fn run_config() -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    let cfg = load_config()?;
    init_logging(&cfg, LogDestination::Stderr)?;
    log_startup(&cfg)?;

    println!("Config file: {}", config_path.display());
    println!();

    let toml = toml::to_string_pretty(&cfg)
        .map_err(|e| FluxError::Config(format!("Failed to serialize config for display: {e}")))?;

    print!("{toml}");

    Ok(())
}

fn print_organize_summary(summary: &OrganizeSummary, dry_run: bool) {
    println!();
    println!("  Organize summary");
    println!("  ────────────────────────────────────");
    if dry_run {
        println!("  Mode:        dry-run");
        println!("  Would move:  {}", summary.dry_run);
    } else {
        println!("  Organized:   {}", summary.organized);
    }
    println!("  Skipped:     {}", summary.skipped);

    if summary.by_rule.is_empty() {
        return;
    }

    println!();
    println!("  By rule:");
    let mut entries: Vec<_> = summary.by_rule.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (rule, count) in entries {
        println!("    {rule}: {count}");
    }
}

fn print_duplicate_report(report: &DuplicateReport, title: &str) {
    println!();
    println!("  {title}");
    println!("  ────────────────────────────────────");
    println!("  Groups:      {}", report.groups.len());
    println!("  Duplicates:  {}", report.duplicate_file_count);
    println!(
        "  Reclaimable: {}",
        format_bytes(report.reclaimable_bytes).green()
    );

    if report.groups.is_empty() {
        return;
    }

    println!();
    for (idx, group) in report.groups.iter().take(10).enumerate() {
        println!(
            "  {}. {} ({} files, {})",
            idx + 1,
            &group.hash[..8.min(group.hash.len())],
            group.files.len(),
            format_bytes(group.size)
        );
        for path in &group.files {
            println!("     {}", path.display());
        }
    }

    if report.groups.len() > 10 {
        println!("  ... and {} more groups", report.groups.len() - 10);
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
