use super::*;
use std::sync::Mutex;

// Exercise the same public contract for every distinct response/handle pair.
// The control deliberately does not implement Debug or Clone.
macro_rules! handle_contract_tests {
    ($module:ident, $handle:ident, $control:ident, $response:ty, $responses:expr) => {
        mod $module {
            use super::*;

            #[derive(Default)]
            struct RecordingControl {
                calls: Mutex<Vec<(AgentServerRequestId, $response)>>,
                error: Option<String>,
            }

            impl $control for RecordingControl {
                fn respond(
                    &self,
                    request_id: &AgentServerRequestId,
                    response: $response,
                ) -> Result<(), String> {
                    self.calls.lock().unwrap().push((request_id.clone(), response));
                    match &self.error {
                        Some(error) => Err(error.clone()),
                        None => Ok(()),
                    }
                }
            }

            fn responses() -> Vec<$response> {
                $responses
            }

            #[test]
            fn forwards_all_responses_with_original_request_ids() {
                let control = Arc::new(RecordingControl::default());
                let mut expected = Vec::new();
                for id in [
                    AgentServerRequestId::Number(-7),
                    AgentServerRequestId::Number(7),
                    AgentServerRequestId::String("7".into()),
                ] {
                    let handle = $handle::new(id.clone(), control.clone());
                    for response in responses() {
                        expected.push((id.clone(), response.clone()));
                        assert_eq!(handle.respond(response), Ok(()));
                    }
                }
                assert_eq!(*control.calls.lock().unwrap(), expected);
            }

            #[test]
            fn clone_keeps_identity_and_routes_to_the_same_control() {
                let control = Arc::new(RecordingControl::default());
                let id = AgentServerRequestId::Number(7);
                let handle = $handle::new(id.clone(), control.clone());
                let cloned = handle.clone();
                assert_eq!(handle, cloned);
                let response = responses().remove(0);
                assert_eq!(cloned.respond(response.clone()), Ok(()));
                assert_eq!(*control.calls.lock().unwrap(), vec![(id, response)]);
            }

            #[test]
            fn equality_requires_both_typed_id_and_control_identity() {
                let control = Arc::new(RecordingControl::default());
                let id = AgentServerRequestId::Number(7);
                let handle = $handle::new(id.clone(), control.clone());
                assert_eq!(handle, $handle::new(id.clone(), control.clone()));
                assert_ne!(
                    handle,
                    $handle::new(id, Arc::new(RecordingControl::default()))
                );
                assert_ne!(
                    handle,
                    $handle::new(AgentServerRequestId::Number(8), control.clone())
                );
                assert_ne!(
                    handle,
                    $handle::new(AgentServerRequestId::String("7".into()), control)
                );
            }

            #[test]
            fn propagates_control_errors_without_retrying() {
                let control = Arc::new(RecordingControl {
                    calls: Mutex::new(Vec::new()),
                    error: Some("response write failed".into()),
                });
                let id = AgentServerRequestId::Number(7);
                let handle = $handle::new(id.clone(), control.clone());
                let response = responses().remove(0);
                assert_eq!(
                    handle.respond(response.clone()),
                    Err("response write failed".into())
                );
                assert_eq!(*control.calls.lock().unwrap(), vec![(id, response)]);
            }

            #[test]
            fn debug_preserves_concrete_name_and_omits_control() {
                let handle = $handle::new(
                    AgentServerRequestId::String("debug-id".into()),
                    Arc::new(RecordingControl::default()),
                );
                assert_eq!(
                    format!("{handle:?}"),
                    concat!(stringify!($handle), " { request_id: String(\"debug-id\"), .. }")
                );
            }

            #[test]
            fn retains_public_trait_bounds() {
                fn assert_traits<T: Clone + fmt::Debug + Eq + Send + Sync>() {}
                assert_traits::<$handle>();
            }
        }
    };
}

handle_contract_tests!(
    command,
    AgentApprovalHandle,
    AgentApprovalControl,
    AgentCommandApprovalChoice,
    vec![
        AgentCommandApprovalChoice::Accept,
        AgentCommandApprovalChoice::AcceptForSession,
        AgentCommandApprovalChoice::Decline,
        AgentCommandApprovalChoice::Cancel,
        AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(vec!["git".into(), "status".into()]),
        AgentCommandApprovalChoice::ApplyNetworkPolicyAmendment(AgentNetworkPolicyAmendment {
            host: "example.test".into(),
            action: AgentNetworkPolicyAction::Allow,
        }),
        AgentCommandApprovalChoice::ApplyNetworkPolicyAmendment(AgentNetworkPolicyAmendment {
            host: "example.test".into(),
            action: AgentNetworkPolicyAction::Deny,
        }),
    ]
);

handle_contract_tests!(
    file,
    AgentFileApprovalHandle,
    AgentFileApprovalControl,
    AgentFileApprovalChoice,
    vec![
        AgentFileApprovalChoice::Accept,
        AgentFileApprovalChoice::AcceptForSession,
        AgentFileApprovalChoice::Decline,
        AgentFileApprovalChoice::Cancel,
    ]
);

handle_contract_tests!(
    user_input,
    AgentUserInputHandle,
    AgentUserInputControl,
    AgentUserInputResponse,
    vec![
        AgentUserInputResponse::default(),
        AgentUserInputResponse {
            answers: vec![AgentUserInputAnswer {
                question_id: "question-1".into(),
                answers: vec!["first answer".into(), "second answer".into()],
            }],
        },
    ]
);

handle_contract_tests!(
    permissions,
    AgentPermissionsApprovalHandle,
    AgentPermissionsApprovalControl,
    AgentPermissionsApprovalChoice,
    vec![
        AgentPermissionsApprovalChoice::AllowOnce,
        AgentPermissionsApprovalChoice::AllowForSession,
        AgentPermissionsApprovalChoice::Decline,
    ]
);
