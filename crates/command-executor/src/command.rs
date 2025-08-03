//! Command type for building executable commands

use async_process::Command as AsyncCommand;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use async_channel::Receiver;

/// A command to be executed
///
/// This is a builder for creating commands that can be converted to `async_process::Command`
/// when needed. Unlike `AsyncCommand`, this type is `Clone` and can be reused multiple times.
#[derive(Debug, Clone)]
pub struct Command {
    /// The program to execute
    program: OsString,
    /// The arguments to pass to the program
    args: Vec<OsString>,
    /// Environment variables to set
    env: HashMap<OsString, OsString>,
    /// Working directory for the command
    current_dir: Option<PathBuf>,
    /// Whether to clear the environment before setting our vars
    env_clear: bool,
    /// Channel to receive stdin input line by line
    stdin_channel: Option<Receiver<String>>,
}

impl Command {
    /// Create a new command for the given program
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            env: HashMap::new(),
            current_dir: None,
            env_clear: false,
            stdin_channel: None,
        }
    }

    /// Add an argument to the command
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    /// Add multiple arguments to the command
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    /// Set an environment variable
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env
            .insert(key.as_ref().to_owned(), val.as_ref().to_owned());
        self
    }

    /// Set multiple environment variables
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (key, val) in vars {
            self.env(key, val);
        }
        self
    }

    /// Clear all environment variables (except those explicitly set)
    pub fn env_clear(&mut self) -> &mut Self {
        self.env_clear = true;
        self
    }

    /// Set the working directory for the command
    pub fn current_dir<P: AsRef<std::path::Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir = Some(dir.as_ref().to_owned());
        self
    }
    
    /// Set a channel to receive stdin input line by line
    pub fn stdin_channel(&mut self, receiver: Receiver<String>) -> &mut Self {
        self.stdin_channel = Some(receiver);
        self
    }

    /// Get the program name
    pub fn get_program(&self) -> &OsStr {
        &self.program
    }

    /// Get the arguments
    pub fn get_args(&self) -> &[OsString] {
        &self.args
    }

    /// Get the environment variables
    pub fn get_envs(&self) -> &HashMap<OsString, OsString> {
        &self.env
    }

    /// Get the current directory
    pub fn get_current_dir(&self) -> Option<&std::path::Path> {
        self.current_dir.as_deref()
    }
    
    /// Check if this command has a stdin channel configured
    pub fn has_stdin_channel(&self) -> bool {
        self.stdin_channel.is_some()
    }
    
    /// Take the stdin channel (consumes it since channels can't be cloned)
    pub fn take_stdin_channel(&mut self) -> Option<Receiver<String>> {
        self.stdin_channel.take()
    }

    /// Prepare this command for execution by converting to an `async_process::Command`
    pub fn prepare(&self) -> AsyncCommand {
        let mut cmd = AsyncCommand::new(&self.program);

        // Add arguments
        cmd.args(&self.args);

        // Set environment
        if self.env_clear {
            cmd.env_clear();
        }
        for (key, val) in &self.env {
            cmd.env(key, val);
        }

        // Set working directory
        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }

        cmd
    }
}

/// Builder pattern helper
impl Command {
    /// Create a builder for this command (for chaining)
    pub fn builder<S: AsRef<OsStr>>(program: S) -> CommandBuilder {
        CommandBuilder(Command::new(program))
    }
}

/// Builder wrapper for more ergonomic command construction
pub struct CommandBuilder(Command);

impl CommandBuilder {
    /// Add an argument
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.0.arg(arg);
        self
    }

    /// Add multiple arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.0.args(args);
        self
    }

    /// Set an environment variable
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.0.env(key, val);
        self
    }

    /// Set the working directory
    pub fn current_dir<P: AsRef<std::path::Path>>(mut self, dir: P) -> Self {
        self.0.current_dir(dir);
        self
    }

    /// Build the command
    pub fn build(self) -> Command {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let cmd = Command::new("echo");
        assert_eq!(cmd.get_program(), "echo");
        assert_eq!(cmd.get_args().len(), 0);
    }

    #[test]
    fn test_command_with_args() {
        let mut cmd = Command::new("ls");
        cmd.arg("-la").arg("/tmp");

        assert_eq!(cmd.get_args().len(), 2);
        assert_eq!(cmd.get_args()[0], "-la");
        assert_eq!(cmd.get_args()[1], "/tmp");
    }

    #[test]
    fn test_command_builder() {
        let cmd = Command::builder("echo")
            .arg("hello")
            .arg("world")
            .env("TEST_VAR", "test_value")
            .current_dir("/tmp")
            .build();

        assert_eq!(cmd.get_program(), "echo");
        assert_eq!(cmd.get_args().len(), 2);
        assert_eq!(cmd.get_args()[0], "hello");
        assert_eq!(cmd.get_args()[1], "world");
        assert_eq!(
            cmd.get_envs().get(OsStr::new("TEST_VAR")),
            Some(&OsString::from("test_value"))
        );
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/tmp")));
    }

    #[test]
    fn test_command_prepare() {
        let cmd = Command::builder("echo").arg("hello").arg("world").build();

        let _async_cmd = cmd.prepare();
        // We can't easily test the AsyncCommand internals, but we can ensure it's created
        // The fact that it compiles and doesn't panic is the test
    }

    #[test]
    fn test_command_clone() {
        let cmd1 = Command::builder("test")
            .arg("arg1")
            .env("KEY", "VALUE")
            .build();

        let cmd2 = cmd1.clone();

        assert_eq!(cmd1.get_program(), cmd2.get_program());
        assert_eq!(cmd1.get_args(), cmd2.get_args());
        assert_eq!(cmd1.get_envs(), cmd2.get_envs());
    }
}
