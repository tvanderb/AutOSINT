use redis::aio::MultiplexedConnection;

/// Stream names for work order priority queues.
pub const STREAM_HIGH: &str = "workorders:high";
pub const STREAM_NORMAL: &str = "workorders:normal";
pub const STREAM_LOW: &str = "workorders:low";

/// Consumer group name for Processors.
pub const CONSUMER_GROUP: &str = "processors";

/// All priority streams in consumption order (high → normal → low).
pub const PRIORITY_STREAMS: &[&str] = &[STREAM_HIGH, STREAM_NORMAL, STREAM_LOW];

/// Redis client for the work order queue.
pub struct QueueClient {
    conn: MultiplexedConnection,
}

impl QueueClient {
    /// Connect to Redis.
    pub async fn connect(redis_url: &str) -> Result<Self, QueueError> {
        tracing::info!("Connecting to Redis");

        let client =
            redis::Client::open(redis_url).map_err(|e| QueueError::Connection(e.to_string()))?;

        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))?;

        let queue_client = Self { conn };
        queue_client.health_check().await?;
        tracing::info!("Redis connection established");

        Ok(queue_client)
    }

    /// Verify the connection is alive (PING).
    pub async fn health_check(&self) -> Result<(), QueueError> {
        let mut conn = self.conn.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        if pong != "PONG" {
            return Err(QueueError::Command(format!(
                "Unexpected PING response: {}",
                pong
            )));
        }
        Ok(())
    }

    /// Initialize streams and consumer groups.
    /// Safe to run on every startup — ignores "already exists" errors.
    pub async fn initialize_streams(&self) -> Result<(), QueueError> {
        tracing::info!("Initializing Redis streams and consumer groups");
        let mut conn = self.conn.clone();

        for stream in PRIORITY_STREAMS {
            // Create stream with consumer group.
            // XGROUP CREATE <stream> <group> $ MKSTREAM
            // $ = only read new messages (not backlog) on first creation.
            let result: Result<String, redis::RedisError> = redis::cmd("XGROUP")
                .arg("CREATE")
                .arg(*stream)
                .arg(CONSUMER_GROUP)
                .arg("$")
                .arg("MKSTREAM")
                .query_async(&mut conn)
                .await;

            match result {
                Ok(_) => {
                    tracing::debug!(
                        stream = *stream,
                        group = CONSUMER_GROUP,
                        "Created consumer group"
                    );
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("BUSYGROUP") {
                        tracing::debug!(
                            stream = *stream,
                            "Consumer group already exists, skipping"
                        );
                    } else {
                        return Err(QueueError::Command(format!(
                            "Failed to create consumer group for {}: {}",
                            stream, e
                        )));
                    }
                }
            }
        }

        tracing::info!("Redis streams initialized");
        Ok(())
    }

    /// Get a clone of the multiplexed connection for direct use.
    #[allow(dead_code)]
    pub fn connection(&self) -> MultiplexedConnection {
        self.conn.clone()
    }

    /// Enqueue a work order message to the appropriate priority stream.
    /// Returns the Redis stream entry ID.
    pub async fn enqueue(
        &self,
        msg: &autosint_common::types::WorkOrderMessage,
        priority: &autosint_common::types::WorkOrderPriority,
    ) -> Result<String, QueueError> {
        let mut conn = self.conn.clone();
        let stream = priority.as_redis_stream();
        let data = serde_json::to_string(msg).map_err(|e| QueueError::Command(e.to_string()))?;

        let entry_id: String = redis::cmd("XADD")
            .arg(stream)
            .arg("*")
            .arg("data")
            .arg(&data)
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        tracing::debug!(
            stream = stream,
            entry_id = %entry_id,
            work_order_id = %msg.work_order_id,
            "Enqueued work order"
        );

        Ok(entry_id)
    }

    /// Dequeue the next work order from any priority stream (high → normal → low).
    /// First checks for pending (previously delivered but unacknowledged) messages,
    /// then reads new messages. Blocks for `block_ms` if no messages available.
    /// Returns `(stream_name, entry_id, message)` or None if no messages available.
    pub async fn dequeue(
        &self,
        consumer_name: &str,
        block_ms: Option<u64>,
    ) -> Result<Option<(String, String, autosint_common::types::WorkOrderMessage)>, QueueError>
    {
        let mut conn = self.conn.clone();

        // First: check for pending messages (ID=0 means re-read our own unacknowledged entries).
        let mut pending_cmd = redis::cmd("XREADGROUP");
        pending_cmd
            .arg("GROUP")
            .arg(CONSUMER_GROUP)
            .arg(consumer_name)
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS");
        for stream in PRIORITY_STREAMS {
            pending_cmd.arg(*stream);
        }
        for _ in PRIORITY_STREAMS {
            pending_cmd.arg("0");
        }

        let pending_result: Option<redis::Value> = pending_cmd
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        if let Some(item) = parse_xreadgroup_response(pending_result)? {
            tracing::debug!(
                consumer = consumer_name,
                stream = %item.0,
                entry_id = %item.1,
                "Reclaimed pending message"
            );
            return Ok(Some(item));
        }

        // No pending messages — read new ones with >.
        let mut cmd = redis::cmd("XREADGROUP");
        cmd.arg("GROUP").arg(CONSUMER_GROUP).arg(consumer_name);

        if let Some(ms) = block_ms {
            cmd.arg("BLOCK").arg(ms);
        }

        cmd.arg("COUNT").arg(1).arg("STREAMS");
        for stream in PRIORITY_STREAMS {
            cmd.arg(*stream);
        }
        for _ in PRIORITY_STREAMS {
            cmd.arg(">");
        }

        let result: Option<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        parse_xreadgroup_response(result)
    }

    /// Acknowledge a message (XACK) after successful processing.
    pub async fn ack(&self, stream: &str, entry_id: &str) -> Result<(), QueueError> {
        let mut conn = self.conn.clone();

        let _: i64 = redis::cmd("XACK")
            .arg(stream)
            .arg(CONSUMER_GROUP)
            .arg(entry_id)
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        Ok(())
    }

    /// Write a heartbeat key for a processor with TTL.
    pub async fn heartbeat(&self, processor_id: &str, ttl_seconds: u64) -> Result<(), QueueError> {
        let mut conn = self.conn.clone();
        let key = format!("processor:{}:heartbeat", processor_id);

        redis::cmd("SET")
            .arg(&key)
            .arg("alive")
            .arg("EX")
            .arg(ttl_seconds)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        Ok(())
    }

    /// Check if a processor heartbeat key exists.
    pub async fn check_heartbeat(&self, processor_id: &str) -> Result<bool, QueueError> {
        let mut conn = self.conn.clone();
        let key = format!("processor:{}:heartbeat", processor_id);

        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Command(e.to_string()))?;

        Ok(exists)
    }

    /// Reclaim stale pending messages (from dead consumers).
    /// Uses XPENDING to find idle entries, then XCLAIM to take ownership.
    pub async fn reclaim_pending(
        &self,
        consumer_name: &str,
        min_idle_ms: u64,
    ) -> Result<Vec<(String, String, autosint_common::types::WorkOrderMessage)>, QueueError> {
        let mut conn = self.conn.clone();
        let mut reclaimed = Vec::new();

        for stream in PRIORITY_STREAMS {
            // XPENDING <stream> <group> IDLE <min_idle_ms> - + 10
            let pending: redis::Value = redis::cmd("XPENDING")
                .arg(*stream)
                .arg(CONSUMER_GROUP)
                .arg("IDLE")
                .arg(min_idle_ms)
                .arg("-")
                .arg("+")
                .arg(10)
                .query_async(&mut conn)
                .await
                .map_err(|e| QueueError::Command(e.to_string()))?;

            let entry_ids = extract_pending_ids(&pending);
            if entry_ids.is_empty() {
                continue;
            }

            // XCLAIM <stream> <group> <consumer> <min_idle_ms> <id> [<id> ...]
            let mut claim_cmd = redis::cmd("XCLAIM");
            claim_cmd
                .arg(*stream)
                .arg(CONSUMER_GROUP)
                .arg(consumer_name)
                .arg(min_idle_ms);

            for id in &entry_ids {
                claim_cmd.arg(id.as_str());
            }

            let claimed: redis::Value = claim_cmd
                .query_async(&mut conn)
                .await
                .map_err(|e| QueueError::Command(e.to_string()))?;

            if let Some(entries) = parse_xclaim_response(stream, &claimed) {
                reclaimed.extend(entries);
            }
        }

        if !reclaimed.is_empty() {
            tracing::info!(count = reclaimed.len(), "Reclaimed pending work orders");
        }

        Ok(reclaimed)
    }
}

/// Parse the XREADGROUP response into (stream, entry_id, message).
/// Redis returns: [[stream_name, [[entry_id, [field, value, ...]]]]]
fn parse_xreadgroup_response(
    value: Option<redis::Value>,
) -> Result<Option<(String, String, autosint_common::types::WorkOrderMessage)>, QueueError> {
    let value = match value {
        Some(v) => v,
        None => return Ok(None),
    };

    // Top level is an array of [stream_name, entries] pairs.
    let streams = match value {
        redis::Value::Array(arr) => arr,
        redis::Value::Nil => return Ok(None),
        _ => return Ok(None),
    };

    for stream_pair in streams {
        let pair = match stream_pair {
            redis::Value::Array(p) => p,
            _ => continue,
        };
        if pair.len() < 2 {
            continue;
        }

        let stream_name = match &pair[0] {
            redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
            _ => continue,
        };

        let entries = match &pair[1] {
            redis::Value::Array(e) => e,
            _ => continue,
        };

        for entry in entries {
            let entry_pair = match entry {
                redis::Value::Array(ep) => ep,
                _ => continue,
            };
            if entry_pair.len() < 2 {
                continue;
            }

            let entry_id = match &entry_pair[0] {
                redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
                _ => continue,
            };

            let fields = match &entry_pair[1] {
                redis::Value::Array(f) => f,
                _ => continue,
            };

            if let Some(msg) = extract_data_field(fields) {
                return Ok(Some((stream_name, entry_id, msg)));
            }
        }
    }

    Ok(None)
}

/// Extract the "data" field from a Redis stream entry's field/value pairs.
fn extract_data_field(fields: &[redis::Value]) -> Option<autosint_common::types::WorkOrderMessage> {
    // Fields are [key, value, key, value, ...]
    let mut i = 0;
    while i + 1 < fields.len() {
        let key = match &fields[i] {
            redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
            _ => {
                i += 2;
                continue;
            }
        };

        if key == "data" {
            let data = match &fields[i + 1] {
                redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
                _ => return None,
            };
            return match serde_json::from_str(&data) {
                Ok(msg) => Some(msg),
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        data = %data,
                        "Failed to deserialize work order message from Redis stream"
                    );
                    None
                }
            };
        }

        i += 2;
    }
    None
}

/// Extract entry IDs from XPENDING response.
/// XPENDING (detailed) returns: [[entry_id, consumer, idle_ms, delivery_count], ...]
fn extract_pending_ids(value: &redis::Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let redis::Value::Array(entries) = value {
        for entry in entries {
            if let redis::Value::Array(fields) = entry {
                if let Some(redis::Value::BulkString(id_bytes)) = fields.first() {
                    ids.push(String::from_utf8_lossy(id_bytes).to_string());
                }
            }
        }
    }
    ids
}

/// Parse XCLAIM response into (stream, entry_id, message) tuples.
/// XCLAIM returns: [[entry_id, [field, value, ...]], ...]
fn parse_xclaim_response(
    stream: &str,
    value: &redis::Value,
) -> Option<Vec<(String, String, autosint_common::types::WorkOrderMessage)>> {
    let entries = match value {
        redis::Value::Array(arr) => arr,
        _ => return None,
    };

    let mut results = Vec::new();
    for entry in entries {
        let entry_pair = match entry {
            redis::Value::Array(ep) => ep,
            _ => continue,
        };
        if entry_pair.len() < 2 {
            continue;
        }

        let entry_id = match &entry_pair[0] {
            redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
            _ => continue,
        };

        let fields = match &entry_pair[1] {
            redis::Value::Array(f) => f,
            _ => continue,
        };

        if let Some(msg) = extract_data_field(fields) {
            results.push((stream.to_string(), entry_id, msg));
        }
    }

    Some(results)
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Redis connection error: {0}")]
    Connection(String),

    #[error("Redis command error: {0}")]
    Command(String),
}

impl From<QueueError> for autosint_common::AutOsintError {
    fn from(e: QueueError) -> Self {
        autosint_common::AutOsintError::Redis(e.to_string())
    }
}
