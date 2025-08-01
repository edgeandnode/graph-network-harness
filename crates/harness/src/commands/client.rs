use anyhow::{Result, anyhow};
use harness::client::DaemonClient;

/// Default daemon port
pub const DEFAULT_DAEMON_PORT: u16 = 9443;

/// Connect to the daemon
pub async fn connect_to_daemon() -> Result<DaemonClient> {
    // TODO: Allow port configuration via env var or config file
    let port = DEFAULT_DAEMON_PORT;

    // Use TLS by default
    match DaemonClient::connect_tls(port, true).await {
        Ok(client) => Ok(client),
        Err(e) => {
            if e.to_string().contains("Connection refused")
                || e.to_string().contains("Connection reset")
            {
                Err(anyhow!(
                    "Cannot connect to harness daemon on port {}.\n\n\
                    Start the daemon with:\n  \
                    harness-executor-daemon\n\n\
                    For more information:\n  \
                    https://github.com/graphprotocol/graph-network-harness#daemon",
                    port
                ))
            } else {
                Err(e)
            }
        }
    }
}
