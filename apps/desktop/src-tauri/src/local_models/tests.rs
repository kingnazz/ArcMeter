use super::*;
use serde_json::{Value, json};

fn database() -> (tempfile::TempDir, Database, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("local.db")).unwrap();
    let device = db.ensure_device("test").unwrap().id;
    (dir, db, device)
}
fn fixture(provider: &str) -> Value {
    serde_json::from_str(match provider {
        "omlx" => include_str!("../../tests/fixtures/local_omlx.json"),
        "ollama" => include_str!("../../tests/fixtures/local_ollama.json"),
        _ => include_str!("../../tests/fixtures/local_lmstudio.json"),
    })
    .unwrap()
}

#[test]
fn endpoints_are_loopback_only_and_never_credentials_or_remote_redirects() {
    for value in [
        "http://localhost:8001/v1",
        "http://127.0.0.1:5432",
        "http://[::1]:1234/v1/",
    ] {
        assert!(providers::endpoint(value).is_ok(), "{value}");
    }
    for value in [
        "https://example.com",
        "http://192.168.1.1:8000",
        "http://localhost.evil:8000",
        "http://user:secret@127.0.0.1:8000",
        "http://localhost:8000?token=secret",
        "http://localhost:8000/#secret",
        "file:///etc/passwd",
        "http://localhost:8000/admin",
    ] {
        assert!(providers::endpoint(value).is_err(), "{value}");
    }
    assert_eq!(
        providers::endpoint("http://localhost:8000/v1")
            .unwrap()
            .host_str(),
        Some("127.0.0.1")
    );
}

#[test]
fn stale_or_restarted_app_never_claims_a_live_runtime_without_redetection() {
    let (_dir, db, device) = database();
    let runtime = runtimes(&db).unwrap().remove(0);
    save_detection(
        &db,
        &runtime,
        Ok(providers::parse_models("omlx", &fixture("omlx"), Utc::now()).unwrap()),
        &device,
    )
    .unwrap();
    assert_eq!(runtimes(&db).unwrap()[0].status, "healthy");
    invalidate_live_state(&db).unwrap();
    assert_eq!(runtimes(&db).unwrap()[0].status, "stale");
    let connection = Connection::open(db.path()).unwrap();
    connection
        .execute(
            "UPDATE local_runtime_state SET status='healthy',checked_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    assert!(runtimes(&db).unwrap().iter().all(|r| r.status == "stale"));
}

#[test]
fn all_provider_fixtures_preserve_only_authoritative_metadata() {
    for provider in providers::PROVIDERS {
        let models = providers::parse_models(provider, &fixture(provider), Utc::now()).unwrap();
        assert_eq!(models.len(), 2);
        let serialized = serde_json::to_string(&models).unwrap();
        assert!(!serialized.contains("never-store"));
        assert!(!serialized.contains("prompt"));
        assert!(!serialized.contains("model_path"));
    }
    let lm = providers::parse_models("lmstudio", &fixture("lmstudio"), Utc::now()).unwrap();
    assert_eq!(lm[0].context_length, Some(8192));
    assert_eq!(lm[1].context_length, None); // trained context is not active context
    let omlx = providers::parse_models("omlx", &fixture("omlx"), Utc::now()).unwrap();
    assert!(omlx.iter().all(|m| m.size_bytes.is_none())); // never use memory estimates
    assert!(providers::parse_models("omlx", &json!({"data":[]}), Utc::now()).is_err());
    assert!(
        providers::parse_models(
            "omlx",
            &json!({"models":[{"id":"/Users/private/model"}]}),
            Utc::now()
        )
        .is_err()
    );
    assert!(providers::identifier("C:\\Users\\secret").is_none());
    assert!(providers::identifier("  /Users/private/model  ").is_none());
    assert!(providers::identifier(" C:/Users/private/model ").is_none());
    assert_eq!(providers::identifier("q:4b").as_deref(), Some("q:4b"));
}

#[test]
fn ollama_running_inventory_multiple_models_and_unload_reload() {
    let now = Utc::now();
    let mut models = providers::parse_models("ollama", &fixture("ollama"), now).unwrap();
    providers::apply_ollama_running(
        &mut models,
        &json!({"models":[{"model":"qwen3:8b","context_length":8192}]}),
        now,
    )
    .unwrap();
    assert_eq!(models.iter().filter(|m| m.loaded == Some(true)).count(), 1);
    assert_eq!(
        models
            .iter()
            .find(|m| m.id == "qwen3:8b")
            .unwrap()
            .context_length,
        Some(8192)
    );
    let (_dir, db, device) = database();
    let old = runtimes(&db).unwrap().remove(1);
    save_detection(&db, &old, Ok(models.clone()), &device).unwrap();
    save_detection(
        &db,
        &runtimes(&db).unwrap().remove(1),
        Ok(models.clone()),
        &device,
    )
    .unwrap();
    assert_eq!(snapshot(&db).unwrap().observations_today, 2); // online + loaded; unchanged poll adds none
    let first_seen = runtimes(&db).unwrap()[1].models[0].first_seen;
    let unloaded = providers::parse_models("ollama", &fixture("ollama"), now).unwrap();
    save_detection(
        &db,
        &runtimes(&db).unwrap().remove(1),
        Ok(unloaded),
        &device,
    )
    .unwrap();
    save_detection(
        &db,
        &runtimes(&db).unwrap().remove(1),
        Ok(models.clone()),
        &device,
    )
    .unwrap();
    assert_eq!(snapshot(&db).unwrap().observations_today, 4);
    save_detection(
        &db,
        &runtimes(&db).unwrap().remove(1),
        Err("Server unavailable"),
        &device,
    )
    .unwrap();
    assert!(
        runtimes(&db).unwrap()[1]
            .models
            .iter()
            .all(|m| m.loaded.is_none())
    );
    save_detection(&db, &runtimes(&db).unwrap().remove(1), Ok(models), &device).unwrap();
    assert_eq!(runtimes(&db).unwrap()[1].models[0].first_seen, first_seen);
    configure(&db, "ollama", true, Some("http://localhost:11435/v1")).unwrap();
    assert!(runtimes(&db).unwrap()[1].models.is_empty());
    configure(&db, "ollama", false, None).unwrap();
    assert_eq!(runtimes(&db).unwrap()[1].status, "disabled");
}

#[test]
fn omlx_history_replay_growth_restart_and_reset_are_idempotent() {
    let (dir, db, device) = database();
    let path = dir.path().join("usage.sqlite3");
    let source = Connection::open(&path).unwrap();
    source.execute_batch("PRAGMA user_version=1; CREATE TABLE model_usage_hourly(timestamp_hour INTEGER,model_id TEXT,
        requests INTEGER,prompt_tokens INTEGER,completion_tokens INTEGER,cached_tokens INTEGER);").unwrap();
    let hour = Utc::now().timestamp() / 3600 * 3600;
    source
        .execute(
            "INSERT INTO model_usage_hourly VALUES(?1,'Qwen',2,100,20,40)",
            [hour],
        )
        .unwrap();
    for _ in 0..100 {
        omlx::collect_path(&db, &device, &path).unwrap();
    }
    let connection = Connection::open(db.path()).unwrap();
    let totals = || {
        connection
            .query_row(
                "SELECT COUNT(*),SUM(total_tokens) FROM usage_events",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .unwrap()
    };
    assert_eq!(totals(), (1, 120));
    source.execute("UPDATE model_usage_hourly SET requests=3,prompt_tokens=150,completion_tokens=30,cached_tokens=60", []).unwrap();
    omlx::collect_path(&db, &device, &path).unwrap();
    assert_eq!(totals(), (1, 180));
    source.execute("UPDATE model_usage_hourly SET requests=1,prompt_tokens=10,completion_tokens=2,cached_tokens=0", []).unwrap();
    omlx::collect_path(&db, &device, &path).unwrap();
    assert_eq!(totals(), (1, 180)); // clearing/resetting history never invents a delta
    source
        .execute(
            "INSERT INTO model_usage_hourly VALUES(?1,'Other',1,5,2,0)",
            [hour],
        )
        .unwrap();
    source
        .execute(
            "INSERT INTO model_usage_hourly VALUES(?1,'/private/secret',1,999,0,0)",
            [hour],
        )
        .unwrap();
    omlx::collect_path(&db, &device, &path).unwrap();
    assert_eq!(totals(), (2, 187));
    let page = crate::sessions::session_page(
        &db,
        crate::sessions::SessionQuery {
            range: "all".into(),
            provider: None,
            search: None,
            sort: None,
            limit: None,
            offset: None,
        },
    )
    .unwrap();
    assert_eq!(page.total_count, 0); // hourly buckets are not sessions
    let dashboard = crate::analytics::dashboard(&db, "all", &Default::default()).unwrap();
    assert_eq!(dashboard.workload.local_tokens, 187);
    assert_eq!(dashboard.workload.cloud_tokens, 0);
    assert_eq!(dashboard.activity.len(), 2);
    assert_eq!(dashboard.metrics.activity_minutes_range, 0);
    assert_eq!(
        crate::cache_efficiency::report(&db, "all", Some("omlx"))
            .unwrap()
            .summary
            .cached_input_tokens,
        60
    );
    source.pragma_update(None, "user_version", 2).unwrap();
    assert!(omlx::collect_path(&db, &device, &path).is_err());
    assert_eq!(totals(), (2, 187));
}

#[tokio::test]
async fn unavailable_and_malformed_local_servers_return_safe_diagnostics() {
    assert!(
        providers::detect("omlx", "http://127.0.0.1:1/v1")
            .await
            .is_err()
    );
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 2048];
        let _ = stream.read(&mut buf);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\nsecret bad format").unwrap();
    });
    let message = providers::detect("lmstudio", &endpoint).await.unwrap_err();
    assert_eq!(message, "Invalid model response");
    assert!(!message.contains("secret"));
    server.join().unwrap();
}

fn mock_server(
    responses: Vec<(&'static str, u16, Value)>,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for (path, code, body) in responses {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(error) => panic!("mock server unavailable: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let len = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..len])
                    .starts_with(&format!("GET {path} HTTP/1.1"))
            );
            let body = body.to_string();
            write!(stream,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (address, worker)
}

#[tokio::test]
async fn discovers_all_runtime_apis_on_override_ports_including_empty_omlx_and_legacy_lmstudio() {
    let (address, worker) = mock_server(vec![
        (
            "/v1/models",
            200,
            json!({"data":[{"id":"Qwen","owned_by":"omlx"}]}),
        ),
        ("/v1/models/status", 200, fixture("omlx")),
    ]);
    assert_eq!(
        providers::detect("omlx", &format!("{address}/v1"))
            .await
            .unwrap()
            .len(),
        2
    );
    worker.join().unwrap();
    let (address, worker) = mock_server(vec![
        ("/v1/models", 200, json!({"data":[]})),
        ("/openapi.json", 200, json!({"info":{"title":"oMLX API"}})),
        ("/v1/models/status", 200, json!({"models":[]})),
    ]);
    assert!(
        providers::detect("omlx", &address)
            .await
            .unwrap()
            .is_empty()
    );
    worker.join().unwrap();
    let (address, worker) = mock_server(vec![
        ("/api/version", 200, json!({"version":"test"})),
        ("/api/tags", 200, fixture("ollama")),
        (
            "/api/ps",
            200,
            json!({"models":[{"model":"qwen3:8b","context_length":8192}]}),
        ),
    ]);
    assert_eq!(
        providers::detect("ollama", &address)
            .await
            .unwrap()
            .iter()
            .filter(|m| m.loaded == Some(true))
            .count(),
        1
    );
    worker.join().unwrap();
    let (address, worker) = mock_server(vec![("/api/v1/models", 200, fixture("lmstudio"))]);
    assert_eq!(
        providers::detect("lmstudio", &address).await.unwrap().len(),
        2
    );
    worker.join().unwrap();
    let (address, worker) = mock_server(vec![
        ("/api/v1/models", 404, json!({})),
        (
            "/api/v0/models",
            200,
            json!({"data":[{"id":"gemma","state":"loaded","loaded_context_length":4096}]}),
        ),
    ]);
    assert_eq!(
        providers::detect("lmstudio", &address).await.unwrap()[0].context_length,
        Some(4096)
    );
    worker.join().unwrap();
    let (address, worker) = mock_server(vec![(
        "/v1/models",
        401,
        json!({"error":"secret should never appear"}),
    )]);
    assert_eq!(
        providers::detect("omlx", &address).await.unwrap_err(),
        "Authentication required; ArcMeter does not read runtime credentials"
    );
    worker.join().unwrap();
}
