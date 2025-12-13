//! Resource limits and systemd integration for services.
//!
//! This module provides resource limit configuration for services, using
//! systemd-run --user for cgroups-based resource management. This enables:
//!
//! - Memory limits per service
//! - CPU limits per service
//! - Clean process cleanup via systemd
//! - Parent slice for aggregate resource limits
//!
//! # Example YAML
//!
//! ```yaml
//! services:
//!   graph-node:
//!     resources:
//!       memory: 2G
//!       cpu: 200%  # 2 cores
//!     target:
//!       type: process
//!       ...
//! ```
//!
//! # Systemd Integration
//!
//! Services are wrapped with `systemd-run --user --scope` which:
//! - Places the service in a user-level cgroup
//! - Applies resource limits via cgroup controllers
//! - Enables clean shutdown via `systemctl --user stop`
//! - Groups related services in a parent slice

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Resource limits for a service.
///
/// All fields are optional - only specified limits are applied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceLimits {
    /// Memory limit (e.g., "512M", "2G", "1024K")
    pub memory: Option<ByteSize>,
    /// CPU limit as percentage (e.g., 100% = 1 core, 200% = 2 cores)
    pub cpu: Option<CpuLimit>,
    /// Custom slice name (default: harness-test.slice)
    pub slice: Option<String>,
}

impl ResourceLimits {
    /// Create empty resource limits (no restrictions).
    pub fn none() -> Self {
        Self::default()
    }

    /// Create resource limits with memory only.
    pub fn with_memory(memory: ByteSize) -> Self {
        Self {
            memory: Some(memory),
            ..Default::default()
        }
    }

    /// Create resource limits with CPU only.
    pub fn with_cpu(cpu: CpuLimit) -> Self {
        Self {
            cpu: Some(cpu),
            ..Default::default()
        }
    }

    /// Set the memory limit.
    pub fn memory(mut self, memory: ByteSize) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Set the CPU limit.
    pub fn cpu(mut self, cpu: CpuLimit) -> Self {
        self.cpu = Some(cpu);
        self
    }

    /// Set the systemd slice.
    pub fn slice(mut self, slice: impl Into<String>) -> Self {
        self.slice = Some(slice.into());
        self
    }

    /// Check if any limits are configured.
    pub fn has_limits(&self) -> bool {
        self.memory.is_some() || self.cpu.is_some()
    }

    /// Generate systemd-run arguments for these limits.
    ///
    /// Returns arguments to pass to `systemd-run --user --scope`.
    pub fn to_systemd_args(&self, service_name: &str) -> Vec<String> {
        let mut args = vec![
            "--user".to_string(),
            "--scope".to_string(),
            format!("--unit=harness-{}.scope", service_name),
        ];

        // Add slice
        let slice = self
            .slice
            .clone()
            .unwrap_or_else(|| "harness-test.slice".to_string());
        args.push(format!("--slice={}", slice));

        // Add memory limit
        if let Some(memory) = &self.memory {
            args.push("-p".to_string());
            args.push(format!("MemoryMax={}", memory.to_bytes()));
        }

        // Add CPU limit
        if let Some(cpu) = &self.cpu {
            // CPUQuota is percentage, e.g., "200%" for 2 cores
            args.push("-p".to_string());
            args.push(format!("CPUQuota={}%", cpu.percentage));
        }

        args
    }

    /// Generate the full systemd-run command prefix.
    pub fn to_systemd_command(&self, service_name: &str) -> Vec<String> {
        let mut cmd = vec!["systemd-run".to_string()];
        cmd.extend(self.to_systemd_args(service_name));
        cmd.push("--".to_string());
        cmd
    }
}

/// Byte size with human-readable parsing.
///
/// Supports formats like "512M", "2G", "1024K", "1073741824" (bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct ByteSize(u64);

impl ByteSize {
    /// Create from bytes.
    pub fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Create from kilobytes.
    pub fn from_kb(kb: u64) -> Self {
        Self(kb * 1024)
    }

    /// Create from megabytes.
    pub fn from_mb(mb: u64) -> Self {
        Self(mb * 1024 * 1024)
    }

    /// Create from gigabytes.
    pub fn from_gb(gb: u64) -> Self {
        Self(gb * 1024 * 1024 * 1024)
    }

    /// Get the size in bytes.
    pub fn to_bytes(&self) -> u64 {
        self.0
    }

    /// Parse from a human-readable string.
    ///
    /// Supports: "512M", "2G", "1024K", "1073741824"
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseError::Empty);
        }

        // Check for suffix
        let (num_str, multiplier) =
            if let Some(n) = s.strip_suffix('K').or_else(|| s.strip_suffix('k')) {
                (n, 1024u64)
            } else if let Some(n) = s.strip_suffix('M').or_else(|| s.strip_suffix('m')) {
                (n, 1024 * 1024)
            } else if let Some(n) = s.strip_suffix('G').or_else(|| s.strip_suffix('g')) {
                (n, 1024 * 1024 * 1024)
            } else if let Some(n) = s.strip_suffix('T').or_else(|| s.strip_suffix('t')) {
                (n, 1024 * 1024 * 1024 * 1024)
            } else {
                (s, 1)
            };

        let num: u64 = num_str
            .parse()
            .map_err(|_| ParseError::InvalidNumber(num_str.to_string()))?;

        Ok(Self(num * multiplier))
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        if bytes >= 1024 * 1024 * 1024 && bytes % (1024 * 1024 * 1024) == 0 {
            write!(f, "{}G", bytes / (1024 * 1024 * 1024))
        } else if bytes >= 1024 * 1024 && bytes % (1024 * 1024) == 0 {
            write!(f, "{}M", bytes / (1024 * 1024))
        } else if bytes >= 1024 && bytes % 1024 == 0 {
            write!(f, "{}K", bytes / 1024)
        } else {
            write!(f, "{}", bytes)
        }
    }
}

impl Serialize for ByteSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ByteSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct ByteSizeVisitor;

        impl<'de> Visitor<'de> for ByteSizeVisitor {
            type Value = ByteSize;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a byte size like '512M', '2G', or '1073741824'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ByteSize::parse(v).map_err(|e| de::Error::custom(e.to_string()))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ByteSize::from_bytes(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v < 0 {
                    Err(de::Error::custom("byte size cannot be negative"))
                } else {
                    Ok(ByteSize::from_bytes(v as u64))
                }
            }
        }

        deserializer.deserialize_any(ByteSizeVisitor)
    }
}

/// CPU limit as a percentage.
///
/// 100% = 1 full CPU core, 200% = 2 cores, 50% = half a core.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuLimit {
    /// Percentage of CPU (100 = 1 core)
    pub percentage: u32,
}

impl CpuLimit {
    /// Create a CPU limit from percentage.
    pub fn from_percentage(percentage: u32) -> Self {
        Self { percentage }
    }

    /// Create a CPU limit from number of cores.
    pub fn from_cores(cores: f32) -> Self {
        Self {
            percentage: (cores * 100.0) as u32,
        }
    }

    /// Parse from a string like "200%", "1.5", "2".
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseError::Empty);
        }

        if let Some(pct_str) = s.strip_suffix('%') {
            // Parse as percentage
            let pct: u32 = pct_str
                .parse()
                .map_err(|_| ParseError::InvalidNumber(pct_str.to_string()))?;
            Ok(Self::from_percentage(pct))
        } else if s.contains('.') {
            // Parse as cores (float)
            let cores: f32 = s
                .parse()
                .map_err(|_| ParseError::InvalidNumber(s.to_string()))?;
            Ok(Self::from_cores(cores))
        } else {
            // Parse as cores (integer)
            let cores: u32 = s
                .parse()
                .map_err(|_| ParseError::InvalidNumber(s.to_string()))?;
            Ok(Self::from_percentage(cores * 100))
        }
    }
}

impl fmt::Display for CpuLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.percentage)
    }
}

impl Serialize for CpuLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CpuLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct CpuLimitVisitor;

        impl<'de> Visitor<'de> for CpuLimitVisitor {
            type Value = CpuLimit;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a CPU limit like '200%', '1.5', or '2'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                CpuLimit::parse(v).map_err(|e| de::Error::custom(e.to_string()))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Interpret as number of cores
                Ok(CpuLimit::from_percentage((v * 100) as u32))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v < 0 {
                    Err(de::Error::custom("CPU limit cannot be negative"))
                } else {
                    Ok(CpuLimit::from_percentage((v * 100) as u32))
                }
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v < 0.0 {
                    Err(de::Error::custom("CPU limit cannot be negative"))
                } else {
                    Ok(CpuLimit::from_cores(v as f32))
                }
            }
        }

        deserializer.deserialize_any(CpuLimitVisitor)
    }
}

// Custom serialization for ResourceLimits
impl Serialize for ResourceLimits {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;

        if let Some(memory) = &self.memory {
            map.serialize_entry("memory", memory)?;
        }
        if let Some(cpu) = &self.cpu {
            map.serialize_entry("cpu", cpu)?;
        }
        if let Some(slice) = &self.slice {
            map.serialize_entry("slice", slice)?;
        }

        map.end()
    }
}

impl<'de> Deserialize<'de> for ResourceLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            memory: Option<ByteSize>,
            cpu: Option<CpuLimit>,
            slice: Option<String>,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(ResourceLimits {
            memory: helper.memory,
            cpu: helper.cpu,
            slice: helper.slice,
        })
    }
}

/// Parse error for resource values.
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    /// Empty input.
    #[error("empty input")]
    Empty,
    /// Invalid number format.
    #[error("invalid number: {0}")]
    InvalidNumber(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_size_parsing() {
        assert_eq!(ByteSize::parse("512").unwrap().to_bytes(), 512);
        assert_eq!(ByteSize::parse("1K").unwrap().to_bytes(), 1024);
        assert_eq!(ByteSize::parse("1k").unwrap().to_bytes(), 1024);
        assert_eq!(
            ByteSize::parse("512M").unwrap().to_bytes(),
            512 * 1024 * 1024
        );
        assert_eq!(
            ByteSize::parse("2G").unwrap().to_bytes(),
            2 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_byte_size_display() {
        assert_eq!(ByteSize::from_gb(2).to_string(), "2G");
        assert_eq!(ByteSize::from_mb(512).to_string(), "512M");
        assert_eq!(ByteSize::from_kb(1024).to_string(), "1M");
        assert_eq!(ByteSize::from_bytes(512).to_string(), "512");
    }

    #[test]
    fn test_byte_size_serde() {
        let yaml = "512M";
        let parsed: ByteSize = serde_yaml::from_str(&format!("\"{}\"", yaml)).unwrap();
        assert_eq!(parsed.to_bytes(), 512 * 1024 * 1024);

        let serialized = serde_yaml::to_string(&parsed).unwrap();
        assert_eq!(serialized.trim(), "512M");
    }

    #[test]
    fn test_cpu_limit_parsing() {
        assert_eq!(CpuLimit::parse("100%").unwrap().percentage, 100);
        assert_eq!(CpuLimit::parse("200%").unwrap().percentage, 200);
        assert_eq!(CpuLimit::parse("50%").unwrap().percentage, 50);
        assert_eq!(CpuLimit::parse("2").unwrap().percentage, 200);
        assert_eq!(CpuLimit::parse("1.5").unwrap().percentage, 150);
    }

    #[test]
    fn test_cpu_limit_display() {
        assert_eq!(CpuLimit::from_percentage(200).to_string(), "200%");
        assert_eq!(CpuLimit::from_cores(1.5).to_string(), "150%");
    }

    #[test]
    fn test_cpu_limit_serde() {
        let yaml = "200%";
        let parsed: CpuLimit = serde_yaml::from_str(&format!("\"{}\"", yaml)).unwrap();
        assert_eq!(parsed.percentage, 200);

        let serialized = serde_yaml::to_string(&parsed).unwrap();
        assert_eq!(serialized.trim(), "200%");
    }

    #[test]
    fn test_resource_limits_serde() {
        let yaml = r#"
memory: 2G
cpu: 200%
"#;

        let limits: ResourceLimits = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            limits.memory.as_ref().unwrap().to_bytes(),
            2 * 1024 * 1024 * 1024
        );
        assert_eq!(limits.cpu.as_ref().unwrap().percentage, 200);
    }

    #[test]
    fn test_systemd_args() {
        let limits = ResourceLimits::none()
            .memory(ByteSize::from_gb(2))
            .cpu(CpuLimit::from_percentage(200));

        let args = limits.to_systemd_args("graph-node");

        assert!(args.contains(&"--user".to_string()));
        assert!(args.contains(&"--scope".to_string()));
        assert!(args.contains(&"--unit=harness-graph-node.scope".to_string()));
        assert!(args.contains(&"--slice=harness-test.slice".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&format!("MemoryMax={}", 2u64 * 1024 * 1024 * 1024)));
        assert!(args.contains(&format!("CPUQuota=200%")));
    }

    #[test]
    fn test_systemd_command() {
        let limits = ResourceLimits::none().memory(ByteSize::from_mb(512));

        let cmd = limits.to_systemd_command("test-service");

        assert_eq!(cmd[0], "systemd-run");
        assert!(cmd.contains(&"--".to_string()));
    }

    #[test]
    fn test_custom_slice() {
        let limits = ResourceLimits::none().slice("my-custom.slice");

        let args = limits.to_systemd_args("test");
        assert!(args.contains(&"--slice=my-custom.slice".to_string()));
    }

    #[test]
    fn test_empty_limits() {
        let limits = ResourceLimits::none();
        assert!(!limits.has_limits());

        let limits = ResourceLimits::none().memory(ByteSize::from_mb(512));
        assert!(limits.has_limits());
    }
}
