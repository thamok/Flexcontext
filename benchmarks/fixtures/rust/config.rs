pub struct ServerConfig { pub port: u16, pub host: String }
/// Parse a TCP port from environment text, rejecting zero.
pub fn parse_port(value: &str) -> Option<u16> { value.parse::<u16>().ok().filter(|port| *port > 0) }
/// Load server configuration from environment variables.
pub fn load_server_config(port: &str, host: &str) -> Option<ServerConfig> { Some(ServerConfig { port: parse_port(port)?, host: host.into() }) }
/// Format a bind address for the configured listener.
pub fn bind_address(config: &ServerConfig) -> String { format!("{}:{}", config.host, config.port) }
pub fn configuration_help() -> &'static str { "server configuration" }
