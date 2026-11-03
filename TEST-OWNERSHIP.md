# Mixed test ownership map

## system-shape

| Original test | Provider | Source |
| --- | --- | --- |
| `every_system_implementation_function_has_at_most_three_parameters` | same | removed |
| `driver_retention_uses_the_same_supply_acquisition_as_service_installation` | `system_supplies_use_the_single_supply_acquisition` (supplies assertions) | original name retains IOMMU/Hub/harness assertions |
| `instance_retirement_precedes_cleanup_and_dynamic_units_declare_hub` | `instance_retirement_precedes_cleanup` (hooks order) | original name retains composition/login/shell assertions |
| `static_pause_and_resume_target_the_same_whole_team` | same | removed |
| `control_state_is_private_and_assembly_owns_operation_queue` | same | removed |
| `lifecycle_dispatch_does_not_own_task_state_or_compensation` | same | removed |
| `control_request_handlers_do_not_own_registration_or_instance_effects` | same | removed |
| `loader_does_not_depend_on_lifecycle_identity_or_composition` | same | removed |
| `identity_authority_does_not_own_control_installation_state` | same | removed |
| `public_clients_do_not_export_wire_data_or_server_bindings` | same | removed |
| `system_components_have_named_ownership_and_no_legacy_classification` | same | removed |
| `control_uses_injected_cross_capability_hooks` | same | removed |
| `system_uses_declarations_not_account_service_implementation` | same | removed |
| `functional_owners_are_private_and_runtime_adapts_them` | same | removed |
| `request_servers_do_not_depend_on_supervisor_or_client_implementations` | same | removed |
| `loader_frame_and_shutdown_keep_their_distinct_steps` | same | removed |
| `ordinary_programs_do_not_depend_on_system_implementation` | removed | original name, rejects `system_server::`, allows neutral `system_support::` |
| `mark_declarations_cannot_bypass_the_registry` | removed | original name |
| `provider_api_and_public_clients_keep_one_way_dependencies` | `system_client_dependencies_stay_one_way` (client manifest/source) | original name retains terminal/router/hub Source assertions |
| `account_has_its_own_task_and_api_and_uses_authorized_control_contract` | `system_runtime_does_not_own_account_or_legacy_modules` | original name retains Source account service/API/client |
| `daemon_is_one_rtc_only_resident_supervision_process` | `system_runtime_does_not_name_daemon` | original name retains composition and daemon runtime |

## control-rpc

| Original test | Provider | Source |
| --- | --- | --- |
| `control_client_builds_typed_request_and_preserves_wait_budget` | same | removed |
| `instance_async_requests_preserve_wire_shape_and_reply_validation` | same | removed |
| `instance_team_uses_control_identity_and_rejects_mismatched_answers` | same | removed |
| `instance_inspect_accepts_retired_identity_but_rejects_invalid_state_or_task` | same | removed |
| `named_task_query_preserves_budget_and_rejects_invalid_or_wrong_source_answers` | same | removed |
| `failed_or_missing_routes_cannot_confirm_release_delivery` | same | removed |
| `a_successful_waiter_reports_delivery_and_all_routes_are_attempted` | same | removed |
| `delivered_failures_cannot_confirm_successful_release` | same | removed |
| `typed_contracts_extract_return_tokens_for_all_control_faces` | provider system control + identity sections | Source account `Request`/`Call` section retained |

Provider manifest drops `account-api`; Source manifest keeps it. Provider projects `client/src/control/client.rs` and `server/src/system/control/endpoint/command_reply.rs` only.

## capability-import

| Original test | Provider | Source |
| --- | --- | --- |
| `watch_rejects_forged_imports_without_mutating_existing_subscription` | same | removed |
| `watch_retires_only_dead_subscription_and_keeps_other_tasks` | same | removed |
| `closed_watch_can_rejoin_same_task_and_path_without_waiting_for_an_event` | same | removed |
| `hub_rejects_third_party_and_self_owned_sensors_without_claiming_device` | removed | original name |
| `hub_valid_owner_exit_vacates_device_for_next_claim` | removed | original name |
| `hub_failed_delivery_preserves_vacant_cell_and_next_request_succeeds` | removed | original name |
| `hub_unsupported_resource_kind_and_malformed_kind_do_not_claim` | removed | original name |
| `hub_dead_owner_replacement_failure_preserves_old_owner_until_vacate` | removed | original name |

Provider projects `server/src/system/operator/watch.rs`; Source keeps `src/service/hub/core` and `src/service/hub/serve` projections.
