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
