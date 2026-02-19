use serde::{Deserialize, Serialize};

/// POST /transcribe request — submit transcription job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscribeRequest {
    pub url: String,
    /// Platform hint (e.g. "youtube"). Auto-detected if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Contextual metadata for speaker identification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TranscribeContext>,
    /// Whether to perform speaker diarization.
    #[serde(default = "default_diarization")]
    pub diarization: bool,
}

fn default_diarization() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscribeContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_participants: Vec<String>,
}

/// POST /transcribe response — job submission confirmation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscribeSubmitResponse {
    pub job_id: String,
}

/// Transcription job status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscribeJobStatus {
    Queued,
    Processing,
    Complete,
    Failed,
}

/// GET /transcribe/{id} response — job status and results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscribeJobResponse {
    pub status: TranscribeJobStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TranscriptionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Completed transcription result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub language_detected: String,
    pub duration: String,
    pub speaker_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TranscribeContext>,
    pub segments: Vec<TranscriptionSegment>,
}

/// A single segment of a transcription.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub start: String,
    pub end: String,
    pub speaker: String,
    pub content: String,
    pub confidence: f64,
}

/// Platform info from GET /platforms.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub name: String,
    pub description: String,
    /// What URL patterns this platform accepts.
    pub accepts: Vec<String>,
}
