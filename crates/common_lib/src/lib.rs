use std::time::Duration;

pub mod ctrlc;
pub mod errors;
pub mod stock_quote;

pub const DATA_REQUEST: &[u8; 4] = b"DATA";
pub const PING_REQUEST: &[u8; 4] = b"PING";
pub const PONG_REQUEST: &[u8; 4] = b"PONG";
pub const STREAM_REQUEST: &str = "STREAM";
pub const OK_REQUEST: &str = "OK\n";

pub const QUOTE_GENERATOR_PERIOD: Duration = Duration::new(2, 0);
pub const PING_WAIT_PERIOD: Duration = Duration::new(5, 0);
pub const PING_SEND_PERIOD: Duration = Duration::new(1, 0);
pub const UDP_SERVER_RECEIVE_PERIOD: Duration = Duration::new(0, 50_000_000);
pub const QUOTES_WAIT_PERIOD: Duration = Duration::new(6, 0);
pub const TCP_CONNECTION_WAIT_PERIOD: Duration = Duration::new(0, 100_000_000);
pub const UDP_CONNECTION_WAIT_PERIOD: Duration = Duration::new(5, 0);

pub const MAX_NUMBER_IGNORED_PING: u16 = 3;