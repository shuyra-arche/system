# Control and App historical test migration

Audit baseline: `31cf6a6e775019a3fc1613dcc604fd3358124fb6`. This maps every
test function in the 15 assigned historical packages to a current owner test,
native scenario, an obsolete contract with a reason, or an explicit gap. A
historical package is not considered complete merely because its old SDK no
longer compiles. Native evidence is named separately from Host evidence.

## Current Host coverage used by this audit

- Control journal/construction/install/Program: `server/tests/control/{construction,installations,programs}.rs`.
- App declaration/catalog policy: `server/tests/app/declarations.rs`.
- Per Instance retirement: `server/tests/control-retirement/gate.rs`, compiling
  production `server/src/system/control/instance/retirement.rs` with scripted
  provider responses. It checks missing provider admission, pending/lost/
  failed/wrong-task replies, cleanup failures and root release retry.
- The seven suite results are Host evidence only. System and handoff runner
  scenarios are separately selected native evidence; see the exact paths below.

`dynamic-units` has a complete current Host replacement in App declarations;
its obsolete manifest, lock and schema-1 fixture have been removed. All other packages below retain at least one native-only or current-owner gap
and their historical source must remain until that gap is moved or explicitly
accepted by the owner.

## Function mapping

Each historical function name below is an individual mapping. “Retired” means
the old interface or internal mechanism no longer exists; it is not a claim
that a similarly named safety property disappeared. A remaining gap is not
deleted to make an inventory look complete.

### `bootstrap-carrier`

| Historical function | Current mapping or reason |
| --- | --- |
| `prepare_boundary_failures_retry_same_handle` | **Gap:** App boot/launch must retain its actual native roots when a startup delivery is pending or fails. No current Host test drives those effects. |
| `prepare_retry_after_success_does_not_republish` | Partial mapping: construction replay is covered by `control::construction::claimed_construction_keeps_grants_and_ignores_deadline_or_owner_exit`; boot carrier publication after a lost reply remains a gap. |
| `release_failure_keeps_carrier_then_releases_once` | Partial mapping: `control-retirement::gate::failed_begin_cleanup_and_root_release_remain_retryable` checks the same retain-until-release safety at the current provider root owner; bootstrap-carrier cleanup itself remains a gap. |
| `every_partial_prepare_is_releasable_without_publishing_later_steps` | **Gap:** no current Host test checks each partial App bootstrap stage or proves later publication was withheld. |
| `schedule_retries_bootstrap_before_later_prepare_steps` | **Gap:** current service bootstrap ordering is only established by production entry/application scenarios. |
| `expired_retry_does_not_publish_and_retains_the_root_for_release` | Partial mapping: construction expiration and retained claims are covered by `claimed_construction_keeps_grants_and_ignores_deadline_or_owner_exit`; boot expiry/root ownership remains a gap. |
| `valid_carrier_returns_verified_creator_and_gateway` | Retired: the old Carrier response/gateway API was removed. Current authority arrives through authenticated caller identity and declared service grants; this exact credential object is no longer a contract. |
| `multiple_children_get_independent_carriers` | Partial native mapping: `server/tests/target/app/pack/handoff.rs` checks independent selected Instance ownership and roots. Separate per-child startup carrier allocation is retired with Carrier. |
| `missing_reference_or_launch_does_not_unseal` | Retired mechanism: old hole unsealing is not part of schema-2 App startup. Current image/catalog admission is covered by `app::declarations::record_decoding_keeps_boot_strings_borrowed_and_rejects_v1_records` and Loader image admission. |
| `client_rejects_wrong_source_short_frame_role_version` | Current bounded-codec mapping: `api/tests/protocol.rs::envelope_rejects_wrong_version_service_and_absent_route`, `unknown_operations_and_failure_codes_do_not_decode_as_success`; native source authentication remains at Ingress. |
| `client_rejects_foreign_kind_permission_and_duplicates` | Partial mapping: native permission-kind/window policy is covered by `app::declarations::native_rights_keep_kind_window_distribution_and_whole_nest_scope`; authenticated caller and duplicate bootstrap registration remain a boot-owner gap. |
| `mark_is_registered_loader_channel` | Retired: the old mark-selected Loader channel is replaced by named Control construction/Instance service faces and current bounded protocol routes. |

### `control-instance`

| Historical functions | Mapping or reason |
| --- | --- |
| `pending_activation_claim_is_idempotent_without_extending_deadline`, `repeated_claim_after_deadline_keeps_the_successful_claim` | Current claim/replay identity: `control::construction::claimed_construction_keeps_grants_and_ignores_deadline_or_owner_exit`; old activation deadline behavior is retired. |
| `unreturned_or_pending_receipts_cannot_be_evicted_to_build_another_instance`, `consumed_receipt_is_retained_until_its_owner_can_receive_completion` | `control::construction::accepted_ids_survive_lost_responses_conflict_and_exhaustion` and `full_construction_ledger_keeps_existing_replay_and_conflict_decisions` retain bounded receipt history. |
| `cancellation_and_retire_require_cleanup_while_stop_does_not` (current test) | Current exact safety counterpart for historical `ruin_waits_for_reclamation_before_completion`, `release_retries_original_receipt_and_never_confirms_a_failed_oust`, and `returned_receipt_waits_for_real_whole_team_completion_and_hook_reclamation`: Stop intent can complete on stopped evidence; cancellation and Retire need retired evidence. |
| `cancellation_tombstone_prevents_late_install_and_is_caller_local` | Current Install tombstone counterpart for late cancellation; old Kernel Claim receipt state is retired. |
| `selected_owner_is_part_of_requester_intent_and_not_receipt_authority` | Current requester/owner split for delegated creation; native selected-owner lifetime is also exercised by `server/tests/target/handoff` scenarios. |
| `selected_creation_completion_remains_historical`, `accepted_ids_survive_lost_responses_conflict_and_exhaustion` | Current retained operation receipts; `control::construction::selected_creation_completion_remains_historical` proves later retirement does not rewrite completed Create. |
| `activation_wait_is_bounded_and_preserves_failure`, `expired_claim_stops_without_embarking`, `embark_claims_once_and_remains_idempotent_after_deadline`, `failed_embark_stops_after_preserving_claim` | Retired old activation/Embark timer protocol. Current Start/Stop are caller-scoped operations with retained receipts; current Journal tests cover replay and evidence-based completion, not old Kernel call timing. |
| `foreign_owner_cannot_observe_or_change_an_instance`, `claimed_live_owner_is_not_consumed_by_denied_task_or_team`, `claimed_owner_dead_non_denied_errors_do_not_consume`, `claimed_owner_dead_observable_team_does_not_consume`, `claimed_control_owner_is_not_consumed` | Partial native mapping: selected-owner auth/death/cancellation is exercised in `server/tests/target/handoff` owner scenarios. The old claim-consumption rules are gone; current Host has no full Instance command owner facade. |
| `busy_debark_is_pending_and_can_be_retried`, `resuming_busy_team_keeps_transition_retriable` | **Native gap:** current Instance freeze/resume calls are Kernel effects; no Host test claims Busy Team transitions. Keep for qualified guest/hardware evidence if those paths are in scope. |
| `retire_dispatch_can_finish_after_kernel_receipt_has_been_consumed`, `legacy_retire_preserves_owner_receipt_for_late_release` | Retired Kernel receipt consumer path; current System operation receipts are System-owned (`control::construction::accepted_ids_survive_lost_responses_conflict_and_exhaustion`). |
| `returned_receipt_is_retried_without_an_owner_reply_or_another_command` | Partial mapping: current serial `Control::progress` advances accepted operations from retained Instance evidence; journal-level evidence is covered by `control::construction::accepted_ids_survive_lost_responses_conflict_and_exhaustion`. No Host test drives Control's autonomous round-to-round progression without reissuing a request. |
| `zero_main_exit_cannot_hide_an_auxiliary_failure`, `reclamation_drains_late_auxiliary_failures_before_oust`, `retire_hook_waits_for_every_team_member_to_exit` | **Native gap:** whole-Team Task reclamation and late auxiliary exits require the current Kernel Task/Team evidence. A Host Journal cannot prove this. |
| `unclaimed_delegated_instance_keeps_receipt_while_owner_is_alive`, `claimed_owner_dead_pruned_absence_consumes_receipt_until_retire_finishes`, `claimed_instance_does_not_inherit_a_child_job_failure`, `expired_unclaimed_instance_cannot_be_claimed` | Old delegated Claim/pruned Kernel job model removed. Current creation owner/replay is covered by `control::construction` plus native selected-owner handoff scenarios; child-job failure/reclamation details remain native. |
| `pruned_receipt_waits_for_retirement_dependencies_before_consumption` | **Gap:** app shutdown dependency ordering is in `Control::close_round`; no current Host owner test exercises dependent Instance draining. Preserve old assertion until native app evidence is explicitly accepted. |
| `missing_instance_and_invalid_transition_have_no_effects`, `only_successful_retire_hook_records_reclamation_time`, `pending_or_failed_retire_hook_cannot_record_reclamation`, `prepare_hook_is_not_reclamation_progress`, `failed_prepare_cannot_be_claimed_or_embarked_before_retirement`, `repeated_retire_completion_cannot_refresh_reclamation_time` | Retired injected hook API and its timestamps. Current Gate’s concrete provider receipt/cleanup/release evidence is tested by `control-retirement`; no hook timestamp contract remains. |
| `full_instance_table_rejects_before_construction`, `failed_construction_does_not_publish_an_instance`, `successful_construction_is_registered_before_return`, `one_owner_can_register_multiple_live_instances` | Partial mapping: bounded Construction/Operation ledgers and exact replay are tested in `control::construction`; `Control::spawn` capacity and real Job publication ordering have no current Host facade. Retain that gap. |
| `claimed_owner_dead_pruned_absence_consumes_receipt_until_retire_finishes` | Old record-pruning mechanism removed. Safety equivalent is that current receipt does not finish until retired evidence (`control::construction::cancellation_and_retire_require_cleanup_while_stop_does_not`); native owner-death cases remain separate. |

### `control-lifetime`

| Historical function | Mapping or reason |
| --- | --- |
| `successful_boot_releases_only_owned_channels`, `partial_failure_rolls_back_immediately_and_keeps_previous_channels`, `reconnect_replaces_and_reclaims_previous_channels` | **Gap:** old channel objects are gone, but current App worker/endpoint partial boot and reconnect ownership still need direct current-owner/native coverage. Do not infer this from protocol codec tests. |
| `descriptor_writers_use_the_declared_slot_and_release_with_the_service` | Retired descriptor-slot writer mechanism; current typed service endpoints are named bounded queues in `app/endpoints.rs`. Any endpoint teardown proof is native App evidence, not this descriptor contract. |
| `on_spawn_readiness_does_not_allocate_a_ready_channel` | Current model allows `Ready::Spawn`; declaration policy is validated by App tests, but proving no unnecessary native signal allocation is a current Instance integration gap. |
| `already_dead_and_poll_live_are_distinguished`, `exit_during_wait_is_waited_and_live_after_deadline_is_unsettled`, `wait_error_does_not_claim_a_live_task_exited` | **Native gap:** current Job observation and Task exit evidence cannot be faithfully host-mocked as Kernel facts. Preserve for target boundary coverage. |

### `control-publisher`

| Historical functions | Mapping or reason |
| --- | --- |
| `duplicate_declarations_in_one_program_are_authorized_once`, `ambiguity_across_distinct_programs_is_denied_before_liveness`, `unrelated_namespace_read_only_and_instance_declarations_are_excluded`, `zero_marks_are_denied`, `missing_row_is_unknown`, `lifecycle_mismatches_are_not_ready`, `real_named_ready_publisher_returns_expected_task`, `stopping_intent_keeps_the_live_identity_until_external_drain_finishes` | **Gap:** old Publisher selection became current Identity Export admission in `Control::export` + `app/endpoints`. No current owner test covers duplicate/ambiguous source, caller Ready/live state, exact namespace entry proof and retaining the live identity while external drain is pending. These are not replaced by Identity revision tests. |
| `publisher_ask_roundtrips_truncates_and_maps_to_state_grant`, `publisher_opcode_does_not_fall_back_to_a_legacy_name` | Retired PublisherAsk/opcode transport; current Export has a dedicated typed endpoint (`identity::export::Call`). Protocol frames are covered in `api/tests/protocol.rs`; export endpoint/caller authority is the separate gap above. |
| `zero_marks_are_explicit_wire_values` | Mark wire representation remains covered by `api/tests/protocol.rs::local_handles_and_full_width_marks_remain_distinct`; old Publisher opcode semantics are retired. |

### `control-query`

| Historical functions | Mapping or reason |
| --- | --- |
| `shared_work_queue_does_not_burn_ids_when_capacity_rejects_admission`, `shared_take_requires_exact_completed_id_and_preserves_result_once` | Current operation identity and bounded retained receipts are tested by `control::construction::accepted_ids_survive_lost_responses_conflict_and_exhaustion` and `full_construction_ledger_keeps_existing_replay_and_conflict_decisions`. |
| `mutation_requests_enter_the_assembly_queue_with_the_native_parent` | Current owner is `Control::command`/Operation Journal; the journal proves admission/replay but not full Ingress-to-assembly effects. **Ingress composition gap.** |
| `shared_queue_rotates_unmatched_work_without_starving_other_jobs` | Retired multi-client shared queue scheduler; current ingress facades each own one bounded service connection and Control serially processes commands. The queue fairness mechanism itself is no longer a contract. |
| `publisher_query_dispatch_uses_marks_and_returns_task`, `named_query_returns_registered_live_task_from_control`, `missing_exited_and_zero_tasks_never_become_successful_facts` | **Gap:** current Control Observe/State/Task query dispatch is not exercised by the Host Control owner suite. Public frame codecs alone do not establish live caller scoping or native task facts. |
| `shutdown_orders_users_before_providers_and_allows_independent_services`, `stopping_and_debarked_users_still_hold_their_providers`, `scene_users_stop_before_other_services_and_do_not_hold_each_other` | Partial current-owner mapping: `control::programs::shutdown_waits_for_live_required_and_retirement_dependents` checks `close_round`'s real declaration-based dependency selection; the retired Scene table behavior is obsolete. Actual Instance Stop effects, debarked-state liveness, and full close-round progression remain unproven at Host/native boundaries. |

### `control-rpc`

| Historical functions | Mapping or reason |
| --- | --- |
| `control_client_builds_typed_request_and_preserves_wait_budget`, `instance_async_requests_preserve_wire_shape_and_reply_validation`, `instance_team_uses_control_identity_and_rejects_mismatched_answers`, `instance_inspect_accepts_retired_identity_but_rejects_invalid_state_or_task`, `named_task_query_preserves_budget_and_rejects_invalid_or_wrong_source_answers`, `typed_contracts_extract_return_tokens_for_all_control_faces` | Old async client/bus API is retired. Current protocol v4 codecs and typed operation IDs are covered by `api/tests/control.rs::control_wire_ids_and_discovery_reply_budget_remain_bounded` and `api/tests/protocol.rs::all_service_families_have_bounded_complete_frames`; authenticated Ingress source/reply correlation remains unproven at Host composition. |
| `failed_or_missing_routes_cannot_confirm_release_delivery`, `a_successful_waiter_reports_delivery_and_all_routes_are_attempted`, `delivered_failures_cannot_confirm_successful_release` | Old multi-route reply fanout was removed. Current `protocol::Control` owns one authenticated connection and a retained return receipt; keep failed-delivery cleanup as an Ingress/native gap rather than claiming the old fanout test applies. |

### `daemon-control`

| Historical functions | Mapping or reason |
| --- | --- |
| `recovery_admission_accepts_only_a_named_supervisor_with_a_retired_target`, `recovery_admission_rejects_dynamic_copy_revocation_and_wrong_retirement`, `recovery_admission_rejects_scope_guard_intent_and_closing` | Partial mapping: current `control::programs::supervision_requires_matching_group_demand_and_unguarded_policy` tests group and guard demand; full Recover authorization/revalidation has no Host Control facade. |
| `effect_recheck_accepts_the_minted_target_but_repeats_authority`, `pre_build_recheck_repeats_intent_caller_and_closing_before_construction` | **Gap:** current construction and Instance native effects revalidate at Control commit; no current Host test covers those effect boundary checks. |
| `closing_stops_the_daemon_before_the_supervised_provider`, `intent_policy_keeps_cleanup_and_recovery_out_of_stop_semantics`, `duplicate_ruin_merges_and_nonrecovery_conflicts_keep_the_request`, `merged_ruin_commits_stop_before_and_after_old_pre_and_denies_recovery` | Partial current-owner mapping: `control::programs::shutdown_releases_dependency_chain_from_leaf_toward_provider` checks the current close-round dependent selection order. Full close progression and physical Stop/Retire effects remain unproven; old request-merge detail is retired. |
| `publisher_identity_survives_accepted_recovery_takeover_while_work_is_denied` | Partial mapping: Program retains its selected Sequence after stop (`control::programs::stopping_preserves_selection_and_does_not_invent_an_attempt`); old publisher takeover authorization is retired with PublisherAsk. |
| `recovery_finish_transitions_to_embark_and_rolls_back_without_instance`, `explicit_ruin_takes_over_recovery_before_and_after_mint_without_dropping_the_team` | Old Recover/Embark state transitions are replaced by current Start/Stop/Retire and selected Instance ownership. `control::construction` covers retained operation intent; physical Team rollback remains a native gap. |
| `observation_scope_is_the_same_static_supervisor_scope`, `supervision_validation_authorizes_only_a_complete_started_group` | Partial mapping: `control::programs::supervision_requires_matching_group_demand_and_unguarded_policy`; caller authorization and complete live group checks remain Control command/target behavior. |

### `dynamic-units`

| Historical function | Current exact mapping |
| --- | --- |
| `package_membership_does_not_instantiate_requested_units` | `app::declarations::selection_orders_startup_but_preserves_requested_units_without_starting_them`: Requested Unit remains in Catalog while startup contains only selected Startup units. |
| `malformed_runtime_selection_and_wait_cycles_are_rejected` | `app::declarations::unresolved_authorities_and_readiness_cycles_are_rejected` covers unresolved requirements, cycles and invalid retirement references; `app::declarations::system_image_kind_must_match_the_declared_supervisor_space` covers the old mismatched image-kind admission. The old schema-1 `scene::programs` helper is retired; the schema-2 Catalog owns selection. |

### `guard-deployment`

| Historical functions | Mapping or reason |
| --- | --- |
| `empty_selection_needs_no_boot_authority`, `guard_authority_must_be_a_live_fetch_only_nole_owned_by_system`, `unselected_domain_never_calls_guard` | **Native gap:** current Spawn/ControlId guard admission uses live Kernel authority; no current Host test proves the native grant kind/owner or absence of calls for unselected domains. |
| `guarded_launch_debarks_guards_resumes_team_then_starts_task`, `resuming_a_paused_domain_keeps_held_without_rearming_guard`, `embark_pre_run_ready_post_completes_the_real_lifecycle` | **Native gap:** start/freeze/resume/deploy order uses Kernel Team calls and cannot be claimed from Program policy tests. |
| `guard_failure_does_not_hold_the_obligation_or_start_the_task`, `team_resume_failure_after_guard_keeps_the_obligation_until_release`, `task_embark_failure_after_guard_keeps_the_obligation_until_release`, `busy_debark_stays_pending_until_the_deadline_then_fails`, `external_retirement_pending_stops_before_pause_unguard_or_slay`, `busy_on_hart_keeps_the_obligation_without_unguard`, `unguard_failure_keeps_the_obligation_and_blocks_destroy`, `successful_release_happens_once_and_retirement_retries_do_not_repeat_it`, `held_obligation_cannot_be_replaced_or_detached` | **Native gap:** guard and whole-Team effect rollback/retirement require a qualified Kernel target. The old guard owner is still a current Unit policy, so these are not obsolete merely because Host tests cannot execute them. |

### `instance-events`

| Historical function | Mapping or reason |
| --- | --- |
| `repeated_native_peeks_emit_one_exit_and_keep_the_native_cause` | Partial current mapping: `server/tests/target/app/pack/tests/mod.rs::rejects_duplicate_start_complete_and_task_events` rejects duplicate Exited in the consumed stream; app/handoff native logs exercise exactly-once event emission. Proving repeated native peeks and exit cause is a target assertion, not parser-only coverage. |
| `retirement_and_readiness_cannot_be_invented_from_control_state` | Partial: `control-retirement` requires provider receipt, cleanup and root release before gate completion; `server/tests/target/app/pack/tests/mod.rs::accepts_interleaved_tasks_and_partial_creation_failure_without_ready` allows valid Bound/Exited/Retired partial creation without Ready and validates ordering. Actual event emission still needs native evidence. |
| `a_failed_delivery_does_not_advance_publication_state` | **Gap:** current publication/export admission and failed-delivery behavior needs the `Control::export` owner test; Identity revision publication alone is insufficient. |

### `namespace-installation`

| Historical function | Mapping or reason |
| --- | --- |
| `same_team_instance_tasks_get_distinct_roots_and_reuse_one_per_key`, `instance_without_team_is_rejected_before_any_system_side_effect`, `unknown_logical_key_does_not_fall_back_to_root_or_another_entry`, `coordinates_are_immutable_and_failed_empty_receipt_does_not_block_another_task` | Partial owner mapping: `operator::roots::root_registration_and_transitions_replay_the_original_commit`, `ready_root_compare_replace_retains_history_and_closing_clears_discovery`, and `operator::namespace::root_entry_zero_tombstones_and_stale_revisions_stay_distinct` verify current Operator UnitRoot/CAS authority. **Gap:** current Control Instance namespace install must prove actual per-instance Task/Team roots, no fallback, retained partial grants and cleanup compensation. Keep the historical cases until a direct current-owner/native scenario covers these. |

### `namespace-retirement`

| Historical function | Mapping or reason |
| --- | --- |
| `retires_the_original_grant_through_the_stable_root` | Partial: Operator root tests retain stable current root/CAS identity; System Installation grant-retirement ordering has no current Host test. |
| `denied_revoke_keeps_the_whole_receipt`, `revoking_keeps_the_receipt_until_the_grant_is_closed` | **Gap:** `control-retirement` retains provider roots, not Operator grants/bindings. Current `Installation::finish` revoke/unbind/unpart failure retention must be tested or preserved as native-only. |
| `unpart_unknown_keeps_the_missing_directory_semantics` | Partial: Operator namespace tests cover tombstones and unknown/stale entries; per-Instance Directory removal/receipt retention remains a Control Installation gap. |

### `retirement-boundary`

| Historical function | Mapping or reason |
| --- | --- |
| `refused_begin_and_poll_timeout_do_not_settle`, `in_flight_pending_is_polled_without_rediscovery_then_completes`, `unknown_reply_keeps_the_declared_requirement_and_retries`, `one_pending_participant_blocks_the_global_gate` | `control-retirement::gate::pending_provider_receipt_and_cleanup_block_stop_until_settled`, `failed_begin_cleanup_and_root_release_remain_retryable`, and `missing_provider_and_transport_poll_failure_keep_the_gate_closed`. These compile the real current Instance Gate and check portable order/retry rules. |
| `static_gate_uses_declarations_not_program_names` | Current Gate builds participants from `Unit.control.retirement` declarations; same suite asserts the declared provider/index lookup. |
| `missing_grant_or_failed_discovery_holds_normal_and_forced_entry` | `missing_provider_and_transport_poll_failure_keep_the_gate_closed` covers an unavailable declared anchor; `failed_resource_discovery_keeps_the_provider_requirement_for_retry` covers failed lookup and retry before any provider call. Old separate normal/forced entry modes do not exist. |
| `different_instance_replies_cannot_discharge_each_others_obligations` | `control-retirement::gate::another_instances_receipt_cannot_discharge_this_instance` proves a receipt for one current Instance does not settle another Instance's Gate. |
| `instance_done_does_not_complete_global_retirement`, `forced_closing_progresses_while_its_controller_is_still_live`, `undeployed_programs_do_not_add_global_requirements`, `normal_and_forced_share_the_same_admission_gate` | Old global forced-retirement coordinator was removed. Per-Instance obligations are tested at the current Gate; there is no current global normal/forced API to preserve. |

### `shutdown-policy`

| Historical function | Mapping or reason |
| --- | --- |
| `continued_liveness_does_not_refresh_idle`, `additions_alone_are_not_progress`, `initial_or_older_reclamation_time_cannot_rewind_installation_baseline`, `cleanup_started_but_not_finalized_is_not_reclamation`, `repeated_static_detach_does_not_publish_new_progress`, `reattaching_after_detach_does_not_hide_the_completed_progress` | **Gap:** current bootstrap deadline renews only on committed progress (`app/boot/launch.rs` and app worker); no current Host test covers Control shutdown progress accounting. Old Static/Dynamic table counters are retired, but only native time/progress evidence can replace them faithfully. |
| `static_reclamation_is_not_hidden_by_new_dynamic_team`, `dynamic_reclamation_is_not_hidden_by_replacement` | **Gap:** current Instance/Team retirement and replacement ordering needs native application evidence; `Program` tests cover sequence selection, not teardown progress. |

### `startup-authority`

| Historical function | Mapping or reason |
| --- | --- |
| `shell_unit_policy_is_narrow_and_image_defaults_bind_owner_and_identity_to_caller`, `invalid_policy_is_rejected_before_any_native_effect` | Partial mapping: `app::declarations::invalid_permissions_and_kernel_identity_domains_fail_before_effects` and `native_rights_keep_kind_window_distribution_and_whole_nest_scope` validate current schema policy. Actual creator/Identity authority is an App boot effect boundary. |
| `policy_entries_are_independent_by_holder_and_replay_grants_no_extra_store`, `replay_at_capacity_reuses_roots_and_new_holder_is_full_before_unseal` | Partial: `control::construction::claimed_construction_keeps_grants_and_ignores_deadline_or_owner_exit` covers stable replay and caller/owner idempotency. App unseal root capacity and independent holder effect counts remain untested. |
| `failed_grant_is_not_registered_and_failed_retire_keeps_roots_retryable`, `failed_delivery_and_failed_revoke_keep_the_same_root_for_retry` | Partial mapping: current `control-retirement` and Supply owner tests preserve roots on failed release/revoke. Startup grant delivery and identity installation compensation remain App native gaps. |

## Export admission: five `identity-publication` assertions

These are included because they belong to the same Control/App queue. The owner
is `Control::export` in `server/src/system/control/mod.rs`, dispatched through
`app/endpoints.rs`, with the API shape in `api/src/identity/mod.rs`. No
current Host test proves this cross-owner path; Identity revision tests must not
be counted as export admission coverage.

| Historical function | Current status |
| --- | --- |
| `identity_export_retry_requires_matching_program_and_delivered_receipt` | Retired contract: current Control Export does not pre-register a grant and consume an `ExistingGrant` receipt. It proves the declared caller/subject/entry and submits one Identity Associate; restoring the old retry API would change the current contract. |
| `identity_export_uses_authoritative_operator_entry_reference` | Pure current owner mapping: `control::publication::export_requires_the_exact_nonroot_operator_entry_reference` tests operator, node id (including Node 0), revision, and Root rejection. It does not prove live namespace lookup. |
| `identity_export_admission_uses_registered_source_and_roster_subtree` | Partial: `control::publication::declared_alias_and_current_subject_are_required_for_export` tests exact alias and matching principal; the full live caller, ready Instance and rooted namespace source checks remain in `Control::export` and are not host proven. |
| `identity_export_admission_preserves_source_owner_and_forged_reply_guards` | Partial: the same current owner test rejects an alias/principal mismatch. Export endpoint caller authentication and forged reply-route behavior remain at the current protocol/Context boundary; no Control Host test claims those effects. |
| `private_lookup_cleanup_keeps_both_resources_and_deduplicates_shared_tokens` | Retired two-token mechanism: current `Layout::resource` returns one Resource backed by the actual task Context cleanup contract, and `Control::export` releases each successfully resolved Resource before Identity Associate. The test-local `locate`/`actual` token pair is not a current contract. Native cleanup/release acknowledgment remains target evidence. |

## First-phase changes and remaining work

- Added a current `Catalog` test for System image execution-kind mismatch. Along
  with the existing selection/cycle tests, this moves both `dynamic-units`
  functions to the schema-2 App owner. Removal of that historical manifest
  should be coordinated with the inventory update.
- Added a current Gate test for missing declared provider admission and a lost
  poll reply; it asserts no root release until the same obligation is retried,
  settled, cleaned and released.
- The most important unmoved gaps in this queue are `Control::export` admission
  and lookup cleanup, per-Instance namespace installation/retirement
  compensation, authenticated Control query/Ingress composition, full
  close-round progression and physical shutdown effects, and native guard/
  whole-Team retirement evidence. Keep the historical packages that still
  contain those assertions.
