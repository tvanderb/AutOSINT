use serde::{Deserialize, Serialize};

use super::EmbeddingError;

const OPENAI_EMBEDDINGS_URL: &str = "https://api.openai.com/v1/embeddings";

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    dimensions: u32,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: EmbeddingUsage,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct EmbeddingUsage {
    total_tokens: u64,
}

#[derive(Deserialize)]
struct OpenAiError {
    error: OpenAiErrorDetail,
}

#[derive(Deserialize)]
struct OpenAiErrorDetail {
    message: String,
}

/// Call the OpenAI /v1/embeddings endpoint.
pub async fn call_openai_embeddings(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    dimensions: u32,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let start = std::time::Instant::now();

    let request = EmbeddingRequest {
        model,
        input: texts,
        dimensions,
    };

    let response = http
        .post(OPENAI_EMBEDDINGS_URL)
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .map_err(|e| EmbeddingError::Http(e.to_string()))?;

    let status = response.status();
    let latency = start.elapsed().as_secs_f64();
    metrics::histogram!("embedding.api.latency").record(latency);

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        let body = response.text().await.unwrap_or_default();
        return Err(EmbeddingError::Auth(format!("{}: {}", status, body)));
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(EmbeddingError::RateLimited { retry_after });
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<OpenAiError>(&body)
            .map(|e| e.error.message)
            .unwrap_or(body);
        return Err(EmbeddingError::Api(format!("{}: {}", status, msg)));
    }

    let body: EmbeddingResponse = response
        .json()
        .await
        .map_err(|e| EmbeddingError::Api(format!("Failed to parse response: {}", e)))?;

    metrics::counter!("embedding.api.tokens").increment(body.usage.total_tokens);

    // Sort by index to maintain input order.
    let mut sorted = body.data;
    sorted.sort_by_key(|d| d.index);

    // Validate dimensions.
    for item in &sorted {
        if item.embedding.len() != dimensions as usize {
            return Err(EmbeddingError::DimensionMismatch {
                expected: dimensions,
                got: item.embedding.len(),
            });
        }
    }

    Ok(sorted.into_iter().map(|d| d.embedding).collect())
}
