use super::*;
use crate::agent::{AgentConfigEdit, AgentConfigErrorKind, AgentConfigWrite};
fn read_value(version: &str) -> Value {
    json!({"config":{"model_verbosity":"low"},"origins":{},"layers":[{"name":{"type":"user","file":"/test/config.toml"},"version":version,"config":{"model_verbosity":"low"}}]})
}
fn write() -> AgentConfigWrite {
    AgentConfigWrite {
        generation: 1,
        cwd: "/tmp/project".into(),
        file_path: "/test/config.toml".into(),
        expected_version: "v1".into(),
        reload_user_config: true,
        edits: vec![AgentConfigEdit {
            key: "model_verbosity".into(),
            value: json!("high"),
        }],
    }
}
#[test]
fn real_manager_config_read_write_and_readback_share_connection() {
    let (manager, spawner) = manager_with_fake();
    let read = manager.read_config("/tmp/project".into());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let config = endpoint.recv();
    assert_eq!(
        config["params"],
        json!({"cwd":"/tmp/project","includeLayers":true})
    );
    endpoint.respond(&config, read_value("v1"));
    let requirements = endpoint.recv();
    assert_eq!(requirements["method"], "configRequirements/read");
    endpoint.respond(&requirements, json!({"requirements":null}));
    assert_eq!(wait_value(&read).unwrap().generation, 1);
    let saved = manager.write_config(write());
    let request = endpoint.recv();
    assert_eq!(request["method"], "config/batchWrite");
    assert_eq!(request["params"]["expectedVersion"], "v1");
    endpoint.respond(&request,json!({"status":"okOverridden","filePath":"/test/config.toml","version":"v2","overriddenMetadata":{"message":"managed","effectiveValue":"low","overridingLayer":{"name":{"type":"system","file":"/etc/config.toml"},"version":"org:1"}}}));
    let reread = endpoint.recv();
    assert_eq!(reread["method"], "config/read");
    endpoint.respond(&reread, read_value("v2"));
    let requirements = endpoint.recv();
    endpoint.respond(
        &requirements,
        json!({"requirements":{"allowedWebSearchModes":["disabled"]}}),
    );
    let saved = wait_value(&saved).unwrap();
    assert_eq!(saved.receipt.status, "okOverridden");
    assert!(saved.readback.unwrap().requirements.is_some());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}
#[test]
fn conflict_payload_survives_manager_and_write_is_not_retried() {
    let (manager, spawner) = manager_with_fake();
    let saved = manager.write_config(write());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    endpoint.send(json!({"id":request["id"],"error":{"code":-32600,"message":"changed","data":{"config_write_error_code":"configVersionConflict","extension":true}}}));
    let error = wait_value(&saved).unwrap_err();
    assert_eq!(error.kind, AgentConfigErrorKind::Conflict);
    assert_eq!(error.data.unwrap()["extension"], true);
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}
#[test]
fn connection_loss_after_write_reports_unknown_outcome_without_retry() {
    let (manager, spawner) = manager_with_fake();
    let saved = manager.write_config(write());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "config/batchWrite");
    manager.inner.fail_generation(1, "connection lost".into());
    let error = wait_value(&saved).unwrap_err();
    assert!(error.outcome_unknown);
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}
