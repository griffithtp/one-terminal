//! Basic OData-style `$filter` parsing. Ported from
//! `apps/app-directory/src/router.ts`'s `applyFilter`.
//!
//! Supported forms (case-insensitive field names):
//!   name eq 'ticker-plant'
//!   appId eq 'chart-viewer'
//!   categories/any(c: c eq 'Analytics')
//!   intents/listensFor/<IntentName>
//!
//! All other expressions fall through and return every app (graceful
//! degradation — matches the TS implementation's behavior).

use crate::types::AppD;

pub fn apply_filter(apps: Vec<AppD>, filter: &str) -> Vec<AppD> {
    let lower = filter.trim().to_lowercase();

    if let Some(v) = extract_eq(&lower, "name") {
        return apps
            .into_iter()
            .filter(|a| a.name.to_lowercase().contains(&v))
            .collect();
    }

    if let Some(v) = extract_eq(&lower, "appid") {
        return apps
            .into_iter()
            .filter(|a| a.app_id.to_lowercase() == v)
            .collect();
    }

    if let Some(v) = extract_categories_any(&lower) {
        return apps
            .into_iter()
            .filter(|a| {
                a.categories
                    .as_ref()
                    .map(|cats| cats.iter().any(|c| c.to_lowercase() == v))
                    .unwrap_or(false)
            })
            .collect();
    }

    if let Some(intent) = extract_intent_listens_for(&lower) {
        return apps
            .into_iter()
            .filter(|a| {
                a.interop
                    .as_ref()
                    .and_then(|i| i.intents.as_ref())
                    .and_then(|intents| intents.listens_for.as_ref())
                    .map(|lf| lf.keys().any(|k| k.to_lowercase() == intent))
                    .unwrap_or(false)
            })
            .collect();
    }

    // Unrecognised filter — return all (graceful degradation).
    apps
}

/// `<field> eq '<value>'` → `Some(value)` if `lower` matches that field.
fn extract_eq(lower: &str, field: &str) -> Option<String> {
    let prefix = format!("{field} eq '");
    if let Some(rest) = lower.strip_prefix(&prefix) {
        if let Some(value) = rest.strip_suffix('\'') {
            if !value.is_empty() && !value.contains('\'') {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_categories_any(lower: &str) -> Option<String> {
    let prefix = "categories/any(c: c eq '";
    let rest = lower.strip_prefix(prefix)?;
    let value = rest.strip_suffix("')")?;
    if value.is_empty() || value.contains('\'') {
        return None;
    }
    Some(value.to_string())
}

fn extract_intent_listens_for(lower: &str) -> Option<String> {
    let prefix = "intents/listensfor/";
    let rest = lower.strip_prefix(prefix)?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(rest.to_string())
}
