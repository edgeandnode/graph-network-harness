use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures_util::stream::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Session management for integration test runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub project_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub test_command: String,
    pub test_result: Option<TestResult>,
    pub exit_code: Option<i32>,
    pub services: HashMap<String, ServiceMetadata>,
    pub failure_analysis: Option<FailureAnalysis>,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetadata {
    pub container_id: String,
    pub log_file: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub final_status: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TestResult {
    Success,
    Failed,
    Timeout,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    pub failed_services: Vec<String>,
    pub probable_cause: String,
    pub critical_errors: Vec<LogEntry>,
    pub affected_services: Vec<String>,
    pub recommendations: Vec<String>,
    pub timeline: Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub service: String,
    pub event_type: EventType,
    pub description: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Started,
    Error,
    Crashed,
    Stopped,
    HealthCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub service: String,
    pub message: String,
    pub raw_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone)]
pub struct LogMatch {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub entry: LogEntry,
    pub context: Vec<String>,
}

/// Configuration for logging system
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub base_dir: PathBuf,
    pub max_file_size: u64,
    pub max_files: usize,
    pub buffer_size: usize,
    pub include_timestamps: bool,
    pub min_level: LogLevel,
    pub cleanup_policy: CleanupPolicy,
}

#[derive(Debug, Clone)]
pub struct CleanupPolicy {
    pub max_age: Duration,
    pub max_sessions: usize,
    pub preserve_failures: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("integration-tests/logs"),
            max_file_size: 50 * 1024 * 1024, // 50MB
            max_files: 5,
            buffer_size: 8192, // 8KB
            include_timestamps: true,
            min_level: LogLevel::Info,
            cleanup_policy: CleanupPolicy::default(),
        }
    }
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            max_age: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            max_sessions: 50,
            preserve_failures: true,
        }
    }
}

/// Session manager for organizing logs by test run
#[derive(Clone)]
pub struct LogSession {
    pub session_id: String,
    pub project_name: String,
    pub session_dir: PathBuf,
    pub metadata: SessionMetadata,
    pub config: LogConfig,
}

impl LogSession {
    /// Remove all session directories (force clean)
    pub async fn cleanup_all_sessions(config: &LogConfig) -> Result<usize> {
        let sessions_dir = config.base_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(0);
        }

        let mut cleaned_count = 0;
        let mut entries = fs::read_dir(&sessions_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let path = entry.path();
                fs::remove_dir_all(&path).await?;
                cleaned_count += 1;
                info!("Force cleaned session: {}", path.display());
            }
        }
        Ok(cleaned_count)
    }
    /// Create a new log session
    pub async fn new(project_name: &str, config: LogConfig) -> Result<Self> {
        let session_id = Self::generate_session_id();
        let session_dir = config.base_dir.join("sessions").join(&session_id);

        // Create session directory
        fs::create_dir_all(&session_dir)
            .await
            .context("Failed to create session directory")?;

        // No symlink needed - sessions are self-contained

        let metadata = SessionMetadata {
            session_id: session_id.clone(),
            project_name: project_name.to_string(),
            start_time: Utc::now(),
            end_time: None,
            test_command: std::env::args().collect::<Vec<_>>().join(" "),
            test_result: None,
            exit_code: None,
            services: HashMap::new(),
            failure_analysis: None,
            is_running: true,
        };

        let session = Self {
            session_id,
            project_name: project_name.to_string(),
            session_dir,
            metadata,
            config,
        };

        // Save initial metadata
        session.save_metadata().await?;

        info!("Created log session: {}", session.session_id);
        Ok(session)
    }

    /// Load existing session
    pub async fn load(session_id: &str, config: LogConfig) -> Result<Self> {
        let session_dir = config.base_dir.join("sessions").join(session_id);

        if !session_dir.exists() {
            anyhow::bail!("Session directory not found: {}", session_dir.display());
        }

        let metadata_path = session_dir.join("session.json");
        let metadata_content = fs::read_to_string(&metadata_path)
            .await
            .context("Failed to read session metadata")?;

        let metadata: SessionMetadata =
            serde_json::from_str(&metadata_content).context("Failed to parse session metadata")?;

        Ok(Self {
            session_id: session_id.to_string(),
            project_name: metadata.project_name.clone(),
            session_dir,
            metadata,
            config,
        })
    }

    /// Find running sessions
    pub async fn find_running_sessions(config: &LogConfig) -> Result<Vec<String>> {
        let sessions = Self::list_sessions(config).await?;
        let mut running_sessions = Vec::new();

        for session_id in sessions {
            if let Ok(session) = Self::load(&session_id, config.clone()).await {
                if session.metadata.is_running {
                    running_sessions.push(session_id);
                }
            }
        }

        Ok(running_sessions)
    }

    /// Get the most recent running session
    pub async fn get_running_session(config: LogConfig) -> Result<Self> {
        let running_sessions = Self::find_running_sessions(&config).await?;
        
        if running_sessions.is_empty() {
            anyhow::bail!("No running sessions found");
        }

        // Return the most recent running session (sessions are sorted by timestamp)
        let session_id = running_sessions.last().unwrap();
        Self::load(session_id, config).await
    }

    /// Generate unique session ID
    fn generate_session_id() -> String {
        let now = Utc::now();
        let uuid = uuid::Uuid::new_v4();
        format!(
            "{}_{}",
            now.format("%Y-%m-%d_%H-%M-%S"),
            &uuid.to_string()[..8]
        )
    }

    /// Get path to service log file
    pub fn get_service_log_path(&self, service_name: &str) -> PathBuf {
        self.session_dir.join(format!("{}.log", service_name))
    }

    /// Add service to session
    pub async fn add_service(&mut self, service_name: &str, container_id: &str) -> Result<()> {
        let service_metadata = ServiceMetadata {
            container_id: container_id.to_string(),
            log_file: format!("{}.log", service_name),
            start_time: Utc::now(),
            end_time: None,
            final_status: "starting".to_string(),
            exit_code: None,
        };

        self.metadata
            .services
            .insert(service_name.to_string(), service_metadata);
        self.save_metadata().await?;

        info!(
            "Added service {} to session {}",
            service_name, self.session_id
        );
        Ok(())
    }

    /// Update service status
    pub async fn update_service_status(
        &mut self,
        service_name: &str,
        status: &str,
        exit_code: Option<i32>,
    ) -> Result<()> {
        if let Some(service) = self.metadata.services.get_mut(service_name) {
            service.final_status = status.to_string();
            service.exit_code = exit_code;
            if status == "exited" || status == "stopped" {
                service.end_time = Some(Utc::now());
            }
            self.save_metadata().await?;
        }
        Ok(())
    }

    /// Finalize session
    pub async fn finalize(
        &mut self,
        test_result: TestResult,
        exit_code: Option<i32>,
    ) -> Result<()> {
        self.metadata.end_time = Some(Utc::now());
        self.metadata.test_result = Some(test_result);
        self.metadata.exit_code = exit_code;
        self.metadata.is_running = false;

        // Perform failure analysis if test failed
        if matches!(test_result, TestResult::Failed) {
            self.metadata.failure_analysis = Some(self.analyze_failures().await?);
        }

        self.save_metadata().await?;
        info!("Finalized session: {}", self.session_id);
        Ok(())
    }

    /// Save metadata to file
    async fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.session_dir.join("session.json");
        let metadata_json = serde_json::to_string_pretty(&self.metadata)
            .context("Failed to serialize session metadata")?;

        fs::write(&metadata_path, metadata_json)
            .await
            .context("Failed to write session metadata")?;

        Ok(())
    }

    /// Analyze failures in the session
    async fn analyze_failures(&self) -> Result<FailureAnalysis> {
        let mut failed_services = Vec::new();
        let mut critical_errors = Vec::new();

        // Find failed services
        for (service_name, service_metadata) in &self.metadata.services {
            if service_metadata.final_status == "exited"
                && service_metadata.exit_code.unwrap_or(0) != 0
            {
                failed_services.push(service_name.clone());
            }
        }

        // Extract critical errors from logs
        for service_name in &failed_services {
            let log_path = self.get_service_log_path(service_name);
            if let Ok(errors) = self.extract_errors_from_log(&log_path).await {
                critical_errors.extend(errors);
            }
        }

        // Determine probable cause
        let probable_cause = self.determine_probable_cause(&failed_services, &critical_errors);

        // Generate recommendations
        let recommendations = self.generate_recommendations(&failed_services, &critical_errors);

        // Create timeline
        let timeline = self
            .create_timeline(&failed_services, &critical_errors)
            .await?;

        Ok(FailureAnalysis {
            failed_services,
            probable_cause,
            critical_errors,
            affected_services: Vec::new(), // TODO: implement service dependency analysis
            recommendations,
            timeline,
        })
    }

    /// Extract errors from log file
    async fn extract_errors_from_log(&self, log_path: &Path) -> Result<Vec<LogEntry>> {
        let mut errors = Vec::new();

        if !log_path.exists() {
            return Ok(errors);
        }

        let file = File::open(log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            if let Some(entry) = self.parse_log_line(&line, log_path) {
                if matches!(entry.level, LogLevel::Error | LogLevel::Fatal) {
                    errors.push(entry);
                }
            }
        }

        Ok(errors)
    }

    /// Parse log line into LogEntry
    fn parse_log_line(&self, line: &str, log_path: &Path) -> Option<LogEntry> {
        let service_name = log_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        // Simple log parsing - in practice, you'd want more sophisticated parsing
        let level = if line.contains("ERROR") {
            LogLevel::Error
        } else if line.contains("FATAL") {
            LogLevel::Fatal
        } else if line.contains("WARN") {
            LogLevel::Warning
        } else if line.contains("INFO") {
            LogLevel::Info
        } else if line.contains("DEBUG") {
            LogLevel::Debug
        } else {
            LogLevel::Info
        };

        // Extract timestamp if present
        let timestamp = Utc::now(); // TODO: parse actual timestamp from log line

        Some(LogEntry {
            timestamp,
            level,
            service: service_name.to_string(),
            message: line.to_string(),
            raw_line: line.to_string(),
        })
    }

    /// Determine probable cause of failure
    fn determine_probable_cause(&self, failed_services: &[String], errors: &[LogEntry]) -> String {
        // Simple heuristics - in practice, you'd want more sophisticated analysis
        if failed_services.contains(&"postgres".to_string()) {
            "Database connection or initialization failure".to_string()
        } else if failed_services.contains(&"chain".to_string()) {
            "Blockchain node startup failure".to_string()
        } else if failed_services.contains(&"graph-node".to_string()) {
            "Graph Node initialization or dependency failure".to_string()
        } else if errors
            .iter()
            .any(|e| e.message.contains("connection refused"))
        {
            "Service connectivity issues".to_string()
        } else if errors.iter().any(|e| e.message.contains("timeout")) {
            "Service startup timeout".to_string()
        } else {
            "Unknown failure - check service logs for details".to_string()
        }
    }

    /// Generate recommendations based on failure analysis
    fn generate_recommendations(
        &self,
        failed_services: &[String],
        errors: &[LogEntry],
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        if failed_services.contains(&"postgres".to_string()) {
            recommendations.push("Check PostgreSQL logs for initialization errors".to_string());
            recommendations.push("Verify database permissions and configuration".to_string());
        }

        if failed_services.contains(&"chain".to_string()) {
            recommendations.push("Check if chain ports are available (8545)".to_string());
            recommendations.push("Verify Anvil/Foundry installation".to_string());
        }

        if errors
            .iter()
            .any(|e| e.message.contains("connection refused"))
        {
            recommendations.push("Check if required services are running".to_string());
            recommendations.push("Verify port mappings and firewall settings".to_string());
        }

        if errors.iter().any(|e| e.message.contains("timeout")) {
            recommendations.push("Increase service startup timeout values".to_string());
            recommendations.push("Check system resources (CPU, memory)".to_string());
        }

        if recommendations.is_empty() {
            recommendations
                .push("Check individual service logs for specific error messages".to_string());
            recommendations.push("Verify Docker daemon is running and accessible".to_string());
        }

        recommendations
    }

    /// Create failure timeline
    async fn create_timeline(
        &self,
        failed_services: &[String],
        errors: &[LogEntry],
    ) -> Result<Vec<TimelineEvent>> {
        let mut timeline = Vec::new();

        // Add service start events
        for (service_name, service_metadata) in &self.metadata.services {
            timeline.push(TimelineEvent {
                timestamp: service_metadata.start_time,
                service: service_name.clone(),
                event_type: EventType::Started,
                description: format!("Service {} started", service_name),
                severity: Severity::Info,
            });

            if let Some(end_time) = service_metadata.end_time {
                let event_type = if failed_services.contains(service_name) {
                    EventType::Crashed
                } else {
                    EventType::Stopped
                };

                timeline.push(TimelineEvent {
                    timestamp: end_time,
                    service: service_name.clone(),
                    event_type,
                    description: format!(
                        "Service {} {} with code {:?}",
                        service_name, service_metadata.final_status, service_metadata.exit_code
                    ),
                    severity: if failed_services.contains(service_name) {
                        Severity::Critical
                    } else {
                        Severity::Info
                    },
                });
            }
        }

        // Add error events
        for error in errors {
            timeline.push(TimelineEvent {
                timestamp: error.timestamp,
                service: error.service.clone(),
                event_type: EventType::Error,
                description: error.message.clone(),
                severity: match error.level {
                    LogLevel::Fatal => Severity::Critical,
                    LogLevel::Error => Severity::Error,
                    LogLevel::Warning => Severity::Warning,
                    _ => Severity::Info,
                },
            });
        }

        // Sort by timestamp
        timeline.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(timeline)
    }

    /// List all sessions
    pub async fn list_sessions(config: &LogConfig) -> Result<Vec<String>> {
        let sessions_dir = config.base_dir.join("sessions");

        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let mut entries = fs::read_dir(&sessions_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(session_id) = entry.file_name().to_str() {
                    sessions.push(session_id.to_string());
                }
            }
        }

        sessions.sort();
        Ok(sessions)
    }

    /// Get the latest (most recent) session
    pub async fn latest(config: LogConfig) -> Result<Self> {
        let sessions = Self::list_sessions(&config).await?;

        if sessions.is_empty() {
            anyhow::bail!("No sessions found");
        }

        // Since sessions are sorted by timestamp (in the session ID), the last one is the most recent
        let latest_session_id = sessions.last().unwrap();
        Self::load(latest_session_id, config).await
    }

    /// Clean up old sessions
    pub async fn cleanup_sessions(config: &LogConfig) -> Result<usize> {
        let sessions = Self::list_sessions(config).await?;
        let mut cleaned_count = 0;

        for session_id in sessions {
            if let Ok(session) = Self::load(&session_id, config.clone()).await {
                let age = Utc::now().signed_duration_since(session.metadata.start_time);
                let should_cleanup =
                    age > chrono::Duration::from_std(config.cleanup_policy.max_age)?;

                if should_cleanup
                    && (!config.cleanup_policy.preserve_failures
                        || !matches!(session.metadata.test_result, Some(TestResult::Failed)))
                {
                    let session_dir = config.base_dir.join("sessions").join(&session_id);
                    fs::remove_dir_all(&session_dir).await?;
                    cleaned_count += 1;
                    info!("Cleaned up session: {}", session_id);
                }
            }
        }

        Ok(cleaned_count)
    }
}

/// Background log streaming from Docker containers to files
pub struct LogStreamer {
    docker: Docker,
    session: LogSession,
    services: HashMap<String, ServiceLogStream>,
    cancellation_token: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl LogStreamer {
    /// Create new log streamer
    pub fn new(docker: Docker, session: LogSession) -> Self {
        Self {
            docker,
            session,
            services: HashMap::new(),
            cancellation_token: CancellationToken::new(),
            tasks: Vec::new(),
        }
    }

    /// Start streaming logs from containers
    pub async fn start(&mut self, containers: Vec<ContainerSummary>) -> Result<()> {
        for container in containers {
            if let Some(names) = &container.names {
                if let Some(name) = names.first() {
                    let service_name = name.trim_start_matches('/').to_string();
                    let container_id = container.id.clone().unwrap_or_default();

                    // Create log stream for service
                    let stream = ServiceLogStream::new(
                        service_name.clone(),
                        container_id.clone(),
                        self.session.get_service_log_path(&service_name),
                        self.session.config.clone(),
                    )
                    .await?;

                    self.services.insert(service_name.clone(), stream);

                    // Start streaming task
                    let task = self.start_service_streaming(&service_name).await?;
                    self.tasks.push(task);
                }
            }
        }

        info!("Started log streaming for {} services", self.services.len());
        Ok(())
    }

    /// Start streaming for a specific service
    async fn start_service_streaming(&mut self, service_name: &str) -> Result<JoinHandle<()>> {
        let service_stream = self
            .services
            .get_mut(service_name)
            .context("Service not found")?;

        let docker = self.docker.clone();
        let container_id = service_stream.container_id.clone();
        let log_path = service_stream.log_path.clone();
        let cancellation_token = self.cancellation_token.clone();
        let service_name = service_name.to_string();

        let task = tokio::spawn(async move {
            if let Err(e) =
                stream_container_logs(docker, container_id, log_path, cancellation_token).await
            {
                error!("Log streaming failed for {}: {}", service_name, e);
            }
        });

        Ok(task)
    }

    /// Stop all log streaming
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping log streaming");

        // Signal cancellation
        self.cancellation_token.cancel();

        // Wait for all tasks to complete
        for task in self.tasks.drain(..) {
            task.await?;
        }

        // Close all log files
        for (_, mut stream) in self.services.drain() {
            stream.close().await?;
        }

        info!("Log streaming stopped");
        Ok(())
    }
}

/// Individual service log stream
pub struct ServiceLogStream {
    pub service_name: String,
    pub container_id: String,
    pub log_path: PathBuf,
    pub config: LogConfig,
}

impl ServiceLogStream {
    /// Create new service log stream
    pub async fn new(
        service_name: String,
        container_id: String,
        log_path: PathBuf,
        config: LogConfig,
    ) -> Result<Self> {
        Ok(Self {
            service_name,
            container_id,
            log_path,
            config,
        })
    }

    /// Close the log stream
    pub async fn close(&mut self) -> Result<()> {
        // Log streams are closed automatically when tasks complete
        Ok(())
    }
}

/// Stream logs from a Docker container to a file
async fn stream_container_logs(
    docker: Docker,
    container_id: String,
    log_path: PathBuf,
    cancellation_token: CancellationToken,
) -> Result<()> {
    let options = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        follow: true,
        timestamps: true,
        ..Default::default()
    };

    let mut stream = docker.logs(&container_id, Some(options));

    // Open log file for writing
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await?;

    while let Some(log_result) = stream.next().await {
        // Check for cancellation
        if cancellation_token.is_cancelled() {
            break;
        }

        match log_result {
            Ok(log_output) => {
                let log_line = log_output.to_string();
                file.write_all(log_line.as_bytes()).await?;
                file.flush().await?;
            }
            Err(e) => {
                warn!("Error reading log for container {}: {}", container_id, e);
                break;
            }
        }
    }

    Ok(())
}

/// Log analysis utilities
pub struct LogAnalyzer {
    session: LogSession,
}

impl LogAnalyzer {
    /// Create new log analyzer
    pub fn new(session: LogSession) -> Self {
        Self { session }
    }

    /// Search logs for pattern
    pub async fn search_logs(
        &self,
        service_name: Option<&str>,
        pattern: &str,
    ) -> Result<Vec<LogMatch>> {
        let regex = Regex::new(pattern).context("Invalid regex pattern")?;
        let mut matches = Vec::new();

        let services = if let Some(service) = service_name {
            vec![service.to_string()]
        } else {
            self.session.metadata.services.keys().cloned().collect()
        };

        for service in services {
            let log_path = self.session.get_service_log_path(&service);
            if let Ok(service_matches) = self.search_log_file(&log_path, &regex).await {
                matches.extend(service_matches);
            }
        }

        Ok(matches)
    }

    /// Search a single log file
    async fn search_log_file(&self, log_path: &Path, regex: &Regex) -> Result<Vec<LogMatch>> {
        let mut matches = Vec::new();

        if !log_path.exists() {
            return Ok(matches);
        }

        let file = File::open(log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut line_number = 0;

        while let Some(line) = lines.next_line().await? {
            line_number += 1;

            if regex.is_match(&line) {
                if let Some(entry) = self.session.parse_log_line(&line, log_path) {
                    matches.push(LogMatch {
                        file_path: log_path.to_path_buf(),
                        line_number,
                        entry,
                        context: Vec::new(), // TODO: implement context extraction
                    });
                }
            }
        }

        Ok(matches)
    }

    /// Tail logs from file
    pub async fn tail_logs(&self, service_name: &str, lines: usize) -> Result<Vec<String>> {
        let log_path = self.session.get_service_log_path(service_name);

        if !log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&log_path).await?;
        let reader = BufReader::new(file);
        let mut all_lines = Vec::new();

        let mut lines_reader = reader.lines();
        while let Some(line) = lines_reader.next_line().await? {
            all_lines.push(line);
        }

        // Return last N lines
        let start_idx = if all_lines.len() > lines {
            all_lines.len() - lines
        } else {
            0
        };

        Ok(all_lines[start_idx..].to_vec())
    }
}
