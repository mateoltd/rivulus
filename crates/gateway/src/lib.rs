//! `gateway` - WebSocket shards, cluster, compression, session.
#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod close;
pub mod cluster;
pub mod compression;
pub mod metrics;
pub mod opcodes;
pub mod queue;
pub mod session;
pub mod shard;

pub use close::{classify, CloseAction};
pub use cluster::shard_ids;
pub use cluster::ClusterConfig;
pub use cluster::{bucket, ordered_start_list, recommended_shards, Cluster, ClusterAuth};
pub use compression::{ZlibStream, FRAME_CAP, MESSAGE_CAP, ZSYNC_FLUSH};
pub use metrics::{Metrics, UnknownCounter};
pub use opcodes::Opcode;
pub use queue::SendQueue;
pub use session::{MemorySessionStore, Session, SessionStore};
pub use shard::{backoff_delay, invalid_session_exit, map_close_to_exit, parse_hello_interval};
pub use shard::{connect_url, jittered_heartbeat_delay, shard_for_guild};
pub use shard::{random_fract, resolve_url};
pub use shard::{
    AssembledChunk, Bootstrap, ChunkAssembler, ExpiredChunk, HeartbeatPayload, IdentifyPayload,
    InMemoryQueue, Queue, ResumePayload, Shard, ShardConfig, ShardEvent, ShardExit, ShardId,
    ShardMessenger, ShardStrategy, CHUNK_TIMEOUT, HELLO_TIMEOUT, MAX_RECONNECT_ATTEMPTS,
    PRODUCTION_BASE, READY_TIMEOUT,
};
