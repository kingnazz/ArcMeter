//! Read-only runtime adapters. No inference, model loads, or credentials.
use super::LocalModel;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use serde_json::Value;
use std::time::Duration;

pub const PROVIDERS: [&str; 3] = ["omlx", "ollama", "lmstudio"];

pub fn default_endpoint(provider: &str) -> Option<&'static str> {
    match provider {
        "omlx" => Some("http://127.0.0.1:8000/v1"),
        "ollama" => Some("http://127.0.0.1:11434"),
        "lmstudio" => Some("http://127.0.0.1:1234"),
        _ => None,
    }
}

pub fn endpoint(value: &str) -> Result<Url, &'static str> {
    let mut url = Url::parse(value).map_err(|_| "Enter an HTTP loopback endpoint")?;
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    let local = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !local
        || url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path().trim_end_matches('/'), "" | "/v1")
    {
        return Err(
            "Use http://localhost:port or a loopback IP, optionally ending in /v1; no credentials or query parameters",
        );
    }
    if url.host_str() == Some("localhost") {
        // Do not resolve arbitrary names or follow a loopback service's redirects.
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| "Invalid endpoint")?;
    }
    url.set_path("");
    Ok(url)
}

// Only identifier metadata is accepted. In particular, never retain model paths.
pub fn identifier(value: &str) -> Option<String> {
    crate::domain::safe_identifier(value)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().and_then(identifier)
}
fn count(value: &Value, key: &str) -> Option<i64> {
    value.get(key)?.as_i64().filter(|v| *v > 0)
}

fn model(id: String, now: DateTime<Utc>) -> LocalModel {
    LocalModel {
        name: id.clone(),
        id,
        loaded: None,
        context_length: None,
        quantization: None,
        parameters: None,
        size_bytes: None,
        first_seen: now,
        last_seen: now,
        last_activity_at: None,
    }
}

pub fn parse_models(
    provider: &str,
    value: &Value,
    now: DateTime<Utc>,
) -> Result<Vec<LocalModel>, &'static str> {
    let key = if provider == "lmstudio_v0" {
        "data"
    } else {
        "models"
    };
    let rows = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or("Unrecognized model response")?;
    if rows.len() > 512 {
        return Err("Model inventory exceeds the safe limit");
    }
    let mut models = Vec::new();
    for row in rows {
        let id = match provider {
            "ollama" => text(row, "model").or_else(|| text(row, "name")),
            "lmstudio" => text(row, "key"),
            _ => text(row, "id"),
        };
        let Some(id) = id else {
            return Err("Model response contains an invalid identifier");
        };
        let mut m = model(id, now);
        match provider {
            "omlx" => {
                m.loaded = row.get("loaded").and_then(Value::as_bool);
                m.context_length = count(row, "max_context_window");
                // estimated_size/actual_size are memory estimates, not weight-file sizes.
                m.last_activity_at = row
                    .get("last_access")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && *v > 0.0 && *v <= now.timestamp() as f64)
                    .and_then(|v| DateTime::from_timestamp(v as i64, 0));
            }
            "ollama" => {
                m.loaded = Some(false);
                m.size_bytes = count(row, "size");
                if let Some(details) = row.get("details") {
                    m.parameters = text(details, "parameter_size");
                    m.quantization = text(details, "quantization_level");
                }
            }
            "lmstudio" => {
                let instances = row
                    .get("loaded_instances")
                    .and_then(Value::as_array)
                    .ok_or("Loaded model state is missing")?;
                m.loaded = Some(!instances.is_empty());
                // Only use an active context when all loaded instances agree.
                let contexts: Vec<i64> = instances
                    .iter()
                    .filter_map(|v| v.get("config").and_then(|v| count(v, "context_length")))
                    .collect();
                if !contexts.is_empty()
                    && contexts.len() == instances.len()
                    && contexts.iter().all(|v| *v == contexts[0])
                {
                    m.context_length = Some(contexts[0]);
                }
                m.size_bytes = count(row, "size_bytes");
                m.parameters = text(row, "params_string");
                m.quantization = row.get("quantization").and_then(|q| text(q, "name"));
            }
            "lmstudio_v0" => {
                m.loaded = match row.get("state").and_then(Value::as_str) {
                    Some("loaded") => Some(true),
                    Some("not-loaded") => Some(false),
                    _ => None,
                };
                m.context_length = count(row, "loaded_context_length");
                m.parameters = text(row, "params_string");
                m.quantization = text(row, "quantization");
            }
            _ => return Err("Unsupported provider"),
        }
        models.push(m);
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

pub fn apply_ollama_running(
    models: &mut Vec<LocalModel>,
    value: &Value,
    now: DateTime<Utc>,
) -> Result<(), &'static str> {
    let running = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or("Running model state unavailable")?;
    if running.len() > 512 {
        return Err("Model inventory exceeds the safe limit");
    }
    for row in running {
        let id = text(row, "model")
            .or_else(|| text(row, "name"))
            .ok_or("Invalid running model identifier")?;
        if !models.iter().any(|m| m.id == id) {
            let single = serde_json::json!({"models":[row]});
            models.extend(parse_models("ollama", &single, now)?);
        }
        if let Some(m) = models.iter_mut().find(|m| m.id == id) {
            m.loaded = Some(true);
            m.context_length = count(row, "context_length");
        }
    }
    Ok(())
}

async fn get(client: &Client, base: &Url, path: &str) -> Result<Value, &'static str> {
    let mut url = base.clone();
    url.set_path(path);
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Server unavailable; check that it is running")?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err("Authentication required; ArcMeter does not read runtime credentials");
    }
    if response.status() == StatusCode::NOT_FOUND {
        return Err("Endpoint not supported");
    }
    if !response.status().is_success() {
        return Err("Server reported an unhealthy response");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Incomplete model response")?
    {
        if bytes.len() + chunk.len() > 1_048_576 {
            return Err("Model response exceeds the safe limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid model response")
}

pub async fn detect(provider: &str, address: &str) -> Result<Vec<LocalModel>, &'static str> {
    let base = endpoint(address)?;
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| "Connection initialization failed")?;
    let now = Utc::now();
    match provider {
        "omlx" => {
            // A generic OpenAI-compatible service on port 8000 is not proof of oMLX.
            let list = get(&client, &base, "/v1/models").await?;
            let rows = list
                .get("data")
                .and_then(Value::as_array)
                .ok_or("Unrecognized oMLX response")?;
            if rows.is_empty() {
                let schema = get(&client, &base, "/openapi.json").await?;
                if schema.pointer("/info/title").and_then(Value::as_str) != Some("oMLX API") {
                    return Err("oMLX identity could not be verified");
                }
            } else if !rows
                .iter()
                .all(|v| v.get("owned_by").and_then(Value::as_str) == Some("omlx"))
            {
                return Err("oMLX identity could not be verified");
            }
            match get(&client, &base, "/v1/models/status").await {
                Ok(value) => parse_models("omlx", &value, now),
                Err(_) => {
                    // Older/restricted servers still supply inventory, never inferred load state.
                    let adapted = serde_json::json!({"models":rows});
                    let mut models = parse_models("omlx", &adapted, now)?;
                    for m in &mut models {
                        m.context_length = rows
                            .iter()
                            .find(|r| text(r, "id").as_ref() == Some(&m.id))
                            .and_then(|r| count(r, "max_model_len"));
                    }
                    Ok(models)
                }
            }
        }
        "ollama" => {
            get(&client, &base, "/api/version")
                .await?
                .get("version")
                .and_then(Value::as_str)
                .ok_or("Ollama identity could not be verified")?;
            let mut models = parse_models("ollama", &get(&client, &base, "/api/tags").await?, now)?;
            apply_ollama_running(&mut models, &get(&client, &base, "/api/ps").await?, now)?;
            Ok(models)
        }
        "lmstudio" => match get(&client, &base, "/api/v1/models").await {
            Ok(value) => parse_models("lmstudio", &value, now),
            Err("Endpoint not supported") => parse_models(
                "lmstudio_v0",
                &get(&client, &base, "/api/v0/models").await?,
                now,
            ),
            Err(message) => Err(message),
        },
        _ => Err("Unsupported provider"),
    }
}
