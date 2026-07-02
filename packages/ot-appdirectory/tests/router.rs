use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use ot_appdirectory::{router, AppStore};

fn app() -> axum::Router {
    router(AppStore::new())
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_ok() {
    let resp = app()
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], "2.2");
}

#[tokio::test]
async fn list_apps_returns_seed_data() {
    let resp = app()
        .oneshot(Request::builder().uri("/v2/apps").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let apps = body["applications"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
}

#[tokio::test]
async fn filter_by_app_id() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/v2/apps?%24filter=appId%20eq%20%27chart-viewer%27")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let apps = body["applications"].as_array().unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0]["appId"], "chart-viewer");
}

#[tokio::test]
async fn get_app_not_found() {
    let resp = app()
        .oneshot(Request::builder().uri("/v2/apps/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_get_update_delete_roundtrip() {
    let a = app();

    let new_app = json!({
        "appId": "test-app",
        "name": "Test App",
        "type": "web",
        "details": { "url": "http://localhost:9999" }
    });

    let resp = a
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/apps")
                .header("content-type", "application/json")
                .body(Body::from(new_app.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Duplicate insert -> 409
    let resp = a
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/apps")
                .header("content-type", "application/json")
                .body(Body::from(new_app.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Get it back
    let resp = a
        .clone()
        .oneshot(Request::builder().uri("/v2/apps/test-app").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Update
    let updated = json!({
        "appId": "test-app",
        "name": "Test App Renamed",
        "type": "web",
        "details": { "url": "http://localhost:9999" }
    });
    let resp = a
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v2/apps/test-app")
                .header("content-type", "application/json")
                .body(Body::from(updated.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["name"], "Test App Renamed");

    // Delete
    let resp = a
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v2/apps/test-app")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Now gone
    let resp = a
        .oneshot(Request::builder().uri("/v2/apps/test-app").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_engine_binding_rejected() {
    let new_app = json!({
        "appId": "bad-app",
        "name": "Bad App",
        "type": "web",
        "details": { "url": "http://localhost:9999" },
        "engineBindings": {
            "windows": [{ "family": "electron", "version": "999.0.0" }]
        }
    });

    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/apps")
                .header("content-type", "application/json")
                .body(Body::from(new_app.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn engines_catalog_served() {
    let resp = app()
        .oneshot(Request::builder().uri("/v2/engines").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(body["engines"]["windows"].as_array().unwrap().len() >= 3);
}
