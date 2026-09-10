use super::*;

#[test]
fn reads_null_defaults_open_extensions_and_source_versions() {
    let snapshot = decode_snapshot(8, PathBuf::from("/work"), json!({"result":{
        "config":{"model":null,"approval_policy":{"granular":{"rules":true,"sandbox_approval":false,"mcp_elicitations":true}},"web_search":"indexed","future":{"enabled":true}},
        "origins":{"future":{"name":{"type":"futureManaged","name":"org"},"version":"v2"}},
        "layers":[{"name":{"type":"user","file":"/test/config.toml","profile":null},"version":"v1","config":{},"disabledReason":null}]
    }}), json!({"result":{"requirements":null}})).unwrap();
    assert_eq!(snapshot.effective["future"]["enabled"], true);
    assert_eq!(
        snapshot.origin("future.enabled").unwrap().kind(),
        "futureManaged"
    );
    assert_eq!(snapshot.user_layer().unwrap().source.version, "v1");
    assert!(snapshot.requirements.is_none());
    let absent = decode_snapshot(
        8,
        PathBuf::new(),
        json!({"result":{"config":{},"origins":{}}}),
        json!({"result":{}}),
    )
    .unwrap();
    assert!(absent.layers.is_none());
    assert!(absent.requirements.is_none());
}

#[test]
fn errors_preserve_structured_payload_without_retrying() {
    let result = response_result(json!({"error":{"code":-32600,"message":"changed","data":{"config_write_error_code":"configVersionConflict","future":12}}})).unwrap_err();
    assert_eq!(result.kind, AgentConfigErrorKind::Conflict);
    assert_eq!(result.data.unwrap()["future"], 12);
    assert!(!result.outcome_unknown);
    assert_eq!(response_result(json!({"error":{"message":"invalid","data":{"config_write_error_code":"configValidationError"}}})).unwrap_err().kind,AgentConfigErrorKind::Validation);
}

#[test]
fn writes_explicit_edits_with_null_delete_and_version() {
    let write = AgentConfigWrite {
        generation: 4,
        cwd: "/work".into(),
        file_path: "/test/config.toml".into(),
        expected_version: "sha256:abc".into(),
        reload_user_config: true,
        edits: vec![crate::agent::AgentConfigEdit {
            key: "model_verbosity".into(),
            value: Value::Null,
        }],
    };
    let params = write_params(&write).unwrap();
    assert_eq!(
        params["edits"],
        json!([{"keyPath":"model_verbosity","value":null,"mergeStrategy":"replace"}])
    );
    assert_eq!(params["expectedVersion"], "sha256:abc");
    assert_eq!(params["reloadUserConfig"], true);
    assert!(params.get("model").is_none());
}

#[test]
fn overridden_receipt_keeps_effective_value_and_unknown_metadata() {
    let metadata = json!({"effectiveValue":null,"message":"managed","overridingLayer":{"name":{"type":"mdm","domain":"org","key":"config"},"version":"org:1"},"extension":42});
    let receipt = decode_receipt(json!({"result":{"status":"okOverridden","version":"v2","filePath":"/test/config.toml","overriddenMetadata":metadata}})).unwrap();
    assert_eq!(receipt.status, "okOverridden");
    assert_eq!(receipt.overridden, Some(metadata));
}

#[test]
fn profile_pages_consume_disallowed_inheritance_and_reject_cycles() {
    let mut cursors = Vec::new();
    let profiles = super::super::catalog::permission_profile_pages(std::path::Path::new("/work"),|params| {
        cursors.push(params["cursor"].clone());
        Ok(if params["cursor"].is_null() {json!({"result":{"data":[{"id":":workspace","allowed":true,"extends":null,"future":7}],"nextCursor":"next"}})}
            else {json!({"result":{"data":[{"id":"org","allowed":false,"extends":":workspace"}]}})})
    }).unwrap();
    assert_eq!(cursors, vec![Value::Null, json!("next")]);
    assert_eq!(profiles[1].extends.as_deref(), Some(":workspace"));
    assert!(!profiles[1].allowed);
    assert!(
        super::super::catalog::permission_profile_pages(std::path::Path::new("/work"), |_| Ok(
            json!({"result":{"data":[],"nextCursor":"loop"}})
        ))
        .is_err()
    );
}

#[test]
fn malformed_managed_allowlist_is_unavailable_instead_of_unrestricted() {
    assert!(decode_requirements(json!({"allowedWebSearchModes":"live"})).is_err());
    assert!(decode_requirements(json!({"allowedPermissionProfiles":{"danger":"yes"}})).is_err());
    let requirements=decode_requirements(json!({"allowedWebSearchModes":[],"defaultPermissions":":workspace","models":{"newThread":{"model":"org-model","serviceTier":null}},"network":{"allowedDomains":["example.org"]},"futureRequirement":{"enabled":false}})).unwrap();
    assert_eq!(requirements.allowed["web_search"], Vec::<Value>::new());
    assert_eq!(requirements.enforced["model"], "org-model");
    assert_eq!(requirements.raw["futureRequirement"]["enabled"], false);
}

#[test]
fn configuration_semantics_preserve_legacy_reviewer_and_granular_defaults() {
    let policy =
        json!({"granular":{"rules":true,"sandbox_approval":false,"mcp_elicitations":true}});
    let snapshot=decode_snapshot(1,"/work".into(),json!({"result":{"config":{},"origins":{}}}),json!({"result":{"requirements":{"allowedApprovalPolicies":[policy],"allowedApprovalsReviewers":["auto_review"]}}})).unwrap();
    assert!(
        snapshot
            .restriction("approvals_reviewer", &json!("guardian_subagent"))
            .is_none()
    );
    let complete = json!({"granular":{"rules":true,"sandbox_approval":false,"mcp_elicitations":true,"request_permissions":false,"skill_approval":false}});
    assert!(snapshot.restriction("approval_policy", &complete).is_none());
    let broader = json!({"granular":{"rules":true,"sandbox_approval":false,"mcp_elicitations":true,"request_permissions":true,"skill_approval":false}});
    assert!(snapshot.restriction("approval_policy", &broader).is_some());
}

#[test]
fn named_permission_definitions_require_a_resolvable_default_but_allow_inheritance() {
    let read = json!({"result":{"config":{"permissions":{"team":{"extends":":workspace"}}},"origins":{},"layers":[{"name":{"type":"user","file":"/test/config.toml"},"version":"v1","config":{}}]}});
    let snapshot = decode_snapshot(1, "/work".into(), read.clone(), json!({"result":{}})).unwrap();
    assert_eq!(snapshot.profile_parents["team"], ":workspace");
    assert!(
        snapshot
            .restriction("default_permissions", &Value::Null)
            .is_some()
    );
    let mut read = read;
    read["result"]["layers"].as_array_mut().unwrap().push(json!({"name":{"type":"project","dotCodexFolder":"/work/.codex"},"version":"p1","config":{"default_permissions":"team"}}));
    let snapshot = decode_snapshot(1, "/work".into(), read, json!({"result":{}})).unwrap();
    assert!(
        snapshot
            .restriction("default_permissions", &Value::Null)
            .is_none()
    );
}
