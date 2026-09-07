//! Strict connection notification parameter decoding.

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

pub(super) fn required_param_string(message: &Value, field: &str, method: &str) -> Result<String> {
    message
        .get("params")
        .and_then(|params| params.get(field))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{method} 缺少字符串 params.{field}"))
}

pub(super) fn optional_nullable_param_string(
    message: &Value,
    field: &str,
    method: &str,
) -> Result<Option<String>> {
    match message.get("params").and_then(|params| params.get(field)) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{method} 的 params.{field} 必须是字符串或 null"),
        None => Ok(None),
    }
}

pub(super) fn required_nullable_param_string(
    message: &Value,
    field: &str,
    method: &str,
) -> Result<Option<String>> {
    match message.get("params").and_then(|params| params.get(field)) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{method} 的 params.{field} 必须是字符串或 null"),
        None => bail!("{method} 缺少 params.{field}"),
    }
}
