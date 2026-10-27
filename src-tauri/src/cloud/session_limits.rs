//! Distinct plaintext, encrypted transport and browser JSON limits.
pub const LEGACY_BYTES: usize = 8 << 20;
pub const SNAPSHOT_BYTES: usize = 64 << 20;
pub const WIRE_BYTES: usize = 96 << 20;
// Firefox serializes a JSON string inside a protocol JSON response.
pub const BROWSER_BYTES: usize = 144 << 20;
pub const FRAME_HEADER_BYTES: usize = 13;
pub const TOO_LARGE: &str = "登录快照超过 64 MiB，本机数据已保留";
