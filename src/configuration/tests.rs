use super::*;
use crate::agent::{AgentConfigLayer, AgentConfigSource};
use serde_json::json;

fn snapshot(version: &str) -> AgentConfigSnapshot {
    let source = AgentConfigSource {
        metadata: json!({"type":"user","file":"/test/config.toml"}),
        kind: "user".into(),
        path: Some("/test/config.toml".into()),
        name: None,
        profile: None,
        version: version.into(),
    };
    AgentConfigSnapshot {
        generation: 3,
        cwd: "/work".into(),
        effective: json!({"model_verbosity":"low","web_search":"live"}),
        origins: BTreeMap::from([
            ("model_verbosity".into(), source.clone()),
            ("web_search".into(), source.clone()),
        ]),
        layers: Some(vec![AgentConfigLayer {
            source,
            config: json!({"model_verbosity":"low"}),
            disabled_reason: None,
        }]),
        requirements: None,
        value_aliases: Default::default(),
        value_defaults: Default::default(),
        profile_parents: Default::default(),
        required_fields: Default::default(),
    }
}
fn editor() -> ConfigEditor {
    let mut state = ConfigEditor::default();
    let cycle = state.begin_read();
    state.accept_read(cycle, Ok(snapshot("v1")));
    state
}
fn error(kind: AgentConfigErrorKind) -> AgentConfigError {
    AgentConfigError {
        kind,
        message: "failure".into(),
        data: Some(json!({"code":12})),
        outcome_unknown: false,
    }
}

#[test]
fn inherited_and_explicit_edits_do_not_write_unmodified_fields() {
    let mut state = editor();
    assert!(state.value("web_search").is_none());
    state.edit("web_search", json!("cached")).unwrap();
    state.edit("model_verbosity", json!("high")).unwrap();
    state.edit("model_verbosity", json!("low")).unwrap();
    let (_, write) = state.prepare_write(&[]).unwrap();
    assert_eq!(write.edits.len(), 1);
    assert_eq!(write.edits[0].key, "web_search");
    assert_eq!(write.expected_version, "v1");
    assert!(write.reload_user_config);
}

#[test]
fn conflict_preserves_draft_requires_reread_and_explicit_review() {
    let mut state = editor();
    state.edit("model_verbosity", json!("high")).unwrap();
    let (cycle, _) = state.prepare_write(&[]).unwrap();
    state.accept_save(cycle, Err(error(AgentConfigErrorKind::Conflict)));
    assert_eq!(state.edits["model_verbosity"], "high");
    assert!(state.prepare_write(&[]).is_err());
    state.confirm_review();
    assert!(state.prepare_write(&[]).is_err());
    let cycle = state.begin_read();
    state.accept_read(cycle, Ok(snapshot("v2")));
    assert!(state.needs_review);
    state.confirm_review();
    assert_eq!(state.prepare_write(&[]).unwrap().1.expected_version, "v2");
}

#[test]
fn stale_load_and_save_results_cannot_replace_new_context() {
    let mut state = editor();
    let old = state.begin_read();
    let current = state.begin_read();
    assert!(!state.accept_read(old, Ok(snapshot("old"))));
    assert!(state.accept_read(current, Ok(snapshot("new"))));
    assert_eq!(
        state
            .snapshot
            .as_ref()
            .unwrap()
            .user_layer()
            .unwrap()
            .source
            .version,
        "new"
    );
}

#[test]
fn managed_allowlists_forced_values_empty_lists_and_null() {
    let mut state = editor();
    state.snapshot.as_mut().unwrap().requirements = Some(crate::agent::AgentConfigRequirements {
        raw: json!({"allowedWebSearchModes":["disabled","indexed"]}),
        allowed: BTreeMap::from([
            (
                "web_search".into(),
                vec![json!("disabled"), json!("indexed")],
            ),
            ("approval_policy".into(), Vec::new()),
        ]),
        enforced: BTreeMap::from([
            ("model".into(), json!("managed")),
            ("features.remote_control".into(), json!(false)),
        ]),
    });
    assert!(state.edit("web_search", json!("live")).is_err());
    state.edit("web_search", json!("indexed")).unwrap();
    state.edit("web_search", Value::Null).unwrap();
    assert!(state.edit("approval_policy", json!("never")).is_err());
    assert!(state.edit("model", json!("managed")).is_err());
    assert!(state.edit("features.remote_control", json!(true)).is_err());
    assert!(state.edit("model", Value::Null).is_err());
}

#[test]
fn successful_write_readback_failure_keeps_receipt_and_draft() {
    let mut state = editor();
    state.edit("model_verbosity", json!("high")).unwrap();
    let (cycle, _) = state.prepare_write(&[]).unwrap();
    let receipt = AgentConfigReceipt {
        status: "ok".into(),
        version: "v2".into(),
        file_path: "/test/config.toml".into(),
        overridden: None,
    };
    state.accept_save(
        cycle,
        Ok(AgentConfigSaveResult {
            receipt: receipt.clone(),
            readback: Err(error(AgentConfigErrorKind::Connection)),
        }),
    );
    assert_eq!(state.receipt, Some(receipt));
    assert_eq!(state.edits["model_verbosity"], "high");
    assert!(state.prepare_write(&[]).is_err());
}

#[test]
fn effective_override_does_not_claim_current_runtime_changed() {
    let mut state = editor();
    state.edit("model_verbosity", json!("high")).unwrap();
    let (cycle, _) = state.prepare_write(&[]).unwrap();
    let mut readback = snapshot("v2");
    readback.layers.as_mut().unwrap()[0].config["model_verbosity"] = json!("high");
    readback.origins.insert(
        "model_verbosity".into(),
        AgentConfigSource {
            metadata: json!({"type":"project","dotCodexFolder":"/work/.codex"}),
            kind: "project".into(),
            path: Some("/work/.codex/config.toml".into()),
            name: None,
            profile: None,
            version: "project1".into(),
        },
    );
    state.accept_save(
        cycle,
        Ok(AgentConfigSaveResult {
            receipt: AgentConfigReceipt {
                status: "okOverridden".into(),
                version: "v2".into(),
                file_path: "/test/config.toml".into(),
                overridden: Some(json!({"message":"project wins"})),
            },
            readback: Ok(readback),
        }),
    );
    assert!(state.edits.is_empty());
    assert!(state.feedback.as_ref().unwrap().contains("覆盖"));
    assert!(
        state
            .feedback
            .as_ref()
            .unwrap()
            .contains("当前轮次权限不随配置保存改变")
    );
}

#[test]
fn a_selected_profile_in_the_same_file_is_a_distinct_override_source() {
    let mut state = editor();
    state.edit("model_verbosity", json!("high")).unwrap();
    let (cycle, _) = state.prepare_write(&[]).unwrap();
    let mut readback = snapshot("v2");
    readback.layers.as_mut().unwrap()[0].config["model_verbosity"] = json!("high");
    let mut origin = readback.user_layer().unwrap().source.clone();
    origin.profile = Some("org".into());
    origin.metadata["profile"] = json!("org");
    readback.origins.insert("model_verbosity".into(), origin);
    state.accept_save(
        cycle,
        Ok(AgentConfigSaveResult {
            receipt: AgentConfigReceipt {
                status: "ok".into(),
                version: "v2".into(),
                file_path: "/test/config.toml".into(),
                overridden: None,
            },
            readback: Ok(readback),
        }),
    );
    assert!(state.edits.is_empty());
    assert!(state.feedback.unwrap().contains("覆盖"));
}

#[test]
fn a_failed_refresh_never_allows_saving_against_unverified_requirements() {
    let mut state = editor();
    let cycle = state.begin_read();
    state.accept_read(cycle, Err(error(AgentConfigErrorKind::Write)));
    state.edit("model_verbosity", json!("high")).unwrap();
    assert!(state.prepare_write(&[]).is_err());
    let cycle = state.begin_read();
    state.accept_read(cycle, Ok(snapshot("v1")));
    assert!(state.prepare_write(&[]).is_ok());
}

#[test]
fn matching_effective_value_does_not_hide_a_mismatched_file_write() {
    let mut state = editor();
    state.edit("model_verbosity", json!("high")).unwrap();
    let (cycle, _) = state.prepare_write(&[]).unwrap();
    let mut readback = snapshot("v2");
    readback.effective["model_verbosity"] = json!("high");
    state.accept_save(
        cycle,
        Ok(AgentConfigSaveResult {
            receipt: AgentConfigReceipt {
                status: "ok".into(),
                version: "v2".into(),
                file_path: "/test/config.toml".into(),
                overridden: None,
            },
            readback: Ok(readback),
        }),
    );
    assert!(state.needs_review);
    assert_eq!(state.edits["model_verbosity"], "high");
    assert!(state.feedback.unwrap().contains("文件值"));
}

#[test]
fn static_only_changes_do_not_request_runtime_permission_reload() {
    let mut state = editor();
    state.edit("model", json!("model-test")).unwrap();
    let fields = vec![crate::agent::AgentConfigChoiceSet {
        key: "model".into(),
        values: Vec::new(),
        allows_custom_string: true,
        session_static: true,
    }];
    assert!(!state.prepare_write(&fields).unwrap().1.reload_user_config);
    let mut state = editor();
    state.edit("model", json!("model-test")).unwrap();
    state.edit("web_search", json!("cached")).unwrap();
    assert!(state.prepare_write(&fields).unwrap().1.reload_user_config);
}
