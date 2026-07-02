//! `axum::Router` implementing the FDC3 2.2 App Directory REST surface.
//! Ported from `apps/app-directory/src/router.ts` + `index.ts`'s `/health`.

use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use ot_core::engine::OsKey;

use crate::data::AppStore;
use crate::engines;
use crate::filter::apply_filter;
use crate::types::{AppD, AppsListResponse, EngineCatalogResponse, ErrorResponse};

#[derive(Clone)]
pub struct AppState {
    pub store: AppStore,
}

/// Build the App Directory router. Mount at `/v2` for the FDC3 routes; `/health`
/// is mounted at the router root (matches `apps/app-directory/src/index.ts`).
pub fn router(store: AppStore) -> Router {
    let state = AppState { store };

    let v2 = Router::new()
        .route("/engines", get(get_engines))
        .route("/apps", get(list_apps).post(create_app))
        .route(
            "/apps/:app_id",
            get(get_app).put(update_app).delete(delete_app),
        );

    Router::new()
        .route("/health", get(health))
        .nest("/v2", v2)
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "version": "2.2" }))
}

async fn get_engines() -> impl IntoResponse {
    Json(EngineCatalogResponse {
        engines: engines::catalog(),
        message: "OK".into(),
    })
}

#[derive(Deserialize)]
struct ListAppsQuery {
    #[serde(rename = "$filter")]
    filter: Option<String>,
}

async fn list_apps(
    State(state): State<AppState>,
    Query(q): Query<ListAppsQuery>,
) -> impl IntoResponse {
    let mut results = state.store.get_all().await;
    if let Some(filter) = q.filter.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        results = apply_filter(results, filter);
    }
    Json(AppsListResponse {
        applications: results,
        message: "OK".into(),
    })
}

async fn get_app(State(state): State<AppState>, Path(app_id): Path<String>) -> impl IntoResponse {
    match state.store.get_by_id(&app_id).await {
        Some(app) => Json(app).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "AppNotFound".into(),
            }),
        )
            .into_response(),
    }
}

async fn create_app(State(state): State<AppState>, Json(body): Json<AppD>) -> impl IntoResponse {
    if body.details.url.is_empty() || body.app_id.is_empty() || body.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "Missing required fields: appId, name, type, details.url".into(),
            }),
        )
            .into_response();
    }

    if let Some(err) = validate_engine_bindings(&body.engine_bindings) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: err }),
        )
            .into_response();
    }

    if state.store.exists(&body.app_id).await {
        return (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                message: "AppAlreadyExists".into(),
            }),
        )
            .into_response();
    }

    state.store.insert(body.clone()).await;
    (StatusCode::CREATED, Json(body)).into_response()
}

async fn update_app(
    State(state): State<AppState>,
    Path(app_id): Path<String>,
    Json(body): Json<AppD>,
) -> impl IntoResponse {
    if body.details.url.is_empty() || body.app_id.is_empty() || body.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "Missing required fields: appId, name, type, details.url".into(),
            }),
        )
            .into_response();
    }

    if let Some(err) = validate_engine_bindings(&body.engine_bindings) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: err }),
        )
            .into_response();
    }

    if state.store.replace(&app_id, body.clone()).await {
        Json(body).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "AppNotFound".into(),
            }),
        )
            .into_response()
    }
}

async fn delete_app(
    State(state): State<AppState>,
    Path(app_id): Path<String>,
) -> impl IntoResponse {
    if state.store.remove(&app_id).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "AppNotFound".into(),
            }),
        )
            .into_response()
    }
}

/// Validate `engineBindings` against the engine catalog: every entry must
/// resolve to a `(family, version)` pair present in the catalog, and
/// duplicates within a single OS list are rejected. Ported from
/// `router.ts`'s `validateEngineBindings`.
fn validate_engine_bindings(
    bindings: &Option<std::collections::HashMap<OsKey, Vec<crate::types::EngineBinding>>>,
) -> Option<String> {
    let bindings = bindings.as_ref()?;
    let cat = engines::catalog();

    for (os, list) in bindings.iter() {
        let mut seen: HashSet<String> = HashSet::new();
        for binding in list {
            if !engines::is_engine_binding_valid(&cat, *os, binding) {
                return Some(format!(
                    "engineBindings.{}: {}@{} is not in the catalog",
                    os.as_dir(),
                    binding.family,
                    binding.version
                ));
            }
            let key = format!("{}@{}", binding.family, binding.version);
            if !seen.insert(key.clone()) {
                return Some(format!(
                    "engineBindings.{}: duplicate entry {key}",
                    os.as_dir()
                ));
            }
        }
    }
    None
}
