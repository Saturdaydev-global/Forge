use forge::{
    backend::{OpenCode, parse_events},
    providers::Model,
    storage::Store,
};
use std::time::Duration;
use tempfile::TempDir;

fn model(id: &str, cost: &str, access: &str, priority: i64) -> Model {
    Model {
        id: id.into(),
        cost: cost.into(),
        access: access.into(),
        enabled: true,
        source: "manual".into(),
        priority,
    }
}

#[test]
fn never_routes_paid_or_unknown_and_honors_pin_and_subscription() {
    let tmp = TempDir::new().unwrap();
    let store = Store::open(&tmp.path().join("f.sqlite")).unwrap();
    for m in [
        model("vendor/paid", "paid", "api-key", 100),
        model("vendor/unknown", "unknown", "api-key", 99),
        model("vendor/sub", "subscription", "subscription", 0),
        model("opencode/example-free", "free", "anonymous", 10),
    ] {
        store.register_model(&m).unwrap();
    }
    assert_eq!(
        store.route("free", None).unwrap().id,
        "opencode/example-free"
    );
    assert_eq!(
        store.route("subscription-first", None).unwrap().id,
        "vendor/sub"
    );
    assert!(store.route("pinned", Some("vendor/paid")).is_err());
    assert!(store.route("pinned", Some("missing/model")).is_err());
    assert!(
        OpenCode::new(
            "opencode".into(),
            &store.route("subscription-first", None).unwrap(),
            Duration::from_secs(30)
        )
        .is_err()
    );
}

#[test]
fn discovery_cannot_enable_models_or_overwrite_manual_policy() {
    let tmp = TempDir::new().unwrap();
    let mut store = Store::open(&tmp.path().join("f.sqlite")).unwrap();
    store
        .register_model(&model("openrouter/example/test:free", "paid", "api-key", 0))
        .unwrap();
    let catalog = r#"{"models":[{"model_id":"example/test:free","source":"openrouter"},{"model_id":"example/other","source":"nvidia"},{"model_id":"../../bad ; command","source":"openrouter"}]}"#;
    assert_eq!(store.import_free_router(catalog).unwrap(), 1);
    let models = store.models().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(
        models
            .iter()
            .find(|m| m.id == "openrouter/example/test:free")
            .unwrap()
            .cost,
        "paid"
    );
    assert!(store.route("free", None).is_err());
    assert!(store.import_free_router("{}").is_err());
    assert_eq!(store.models().unwrap().len(), 2);
}

#[test]
fn parses_completed_proposals_and_rejects_errors_and_truncation() {
    let events=b"{\"type\":\"text\",\"part\":{\"text\":\"fn answer() -> u8 { 42 }\"}}\n{\"type\":\"step_finish\",\"part\":{\"reason\":\"stop\"}}\n";
    assert!(parse_events(events).unwrap().contains("42"));
    assert!(parse_events(b"{}").is_err());
    assert!(parse_events(b"{\"type\":\"error\"}").is_err());
    assert!(parse_events(&vec![b'x'; 1_048_577]).is_err());
}
