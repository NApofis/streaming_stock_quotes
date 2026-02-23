pub mod stock_quote;
pub mod errors;
pub mod ctrlc;
pub const DATA_REQUEST: &[u8; 4] = b"DATA";
pub const PING_REQUEST: &[u8; 4] = b"PING";
pub const PONG_REQUEST: &[u8; 4] = b"PONG";
pub const STREAM_REQUEST: &str = "STREAM";
