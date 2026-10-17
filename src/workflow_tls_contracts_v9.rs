//! Ordinary code tests over the genuine owned offline TLS/Git fixture. These
//! never masquerade as real-binary keyboard/mouse product journeys.
use crate::owned_pty_processes_test_support as owned_pty_processes_v1;
#[path = "../tests/support/workflow_tls_v9.rs"]
pub(crate) mod workflow_tls_v9;
use workflow_tls_v9::contract_hooks as contracts;

#[test]
fn task54_workflow_edit_v9_tls_connect_refuses_foreign_authority_credentials_and_duplicate_host() {
    contracts::task54_workflow_tls_v58_connect_refuses_foreign_authority_credentials_and_duplicate_host();
}
#[test]
fn task54_workflow_edit_v9_tls_only_exact_git_paths_and_bounded_protocol2_reach_backend() {
    contracts::task54_workflow_tls_v58_only_exact_git_paths_and_bounded_protocol2_reach_backend();
}
#[test]
fn task54_workflow_edit_v9_tls_actual_cgi_bytes_remain_unchanged_and_auth_ambiguous_framing_refuse()
{
    contracts::task54_workflow_tls_v58_actual_cgi_body_is_unchanged_but_auth_and_ambiguous_framing_refuse();
}
#[test]
fn task54_workflow_edit_v9_tls_header_stream_and_count_caps_refuse_before_backend() {
    contracts::task54_workflow_tls_v58_header_stream_and_count_caps_refuse_before_any_backend();
}
#[test]
fn task54_workflow_edit_v9_tls_real_protocol2_lsrefs_and_create_only_push_change_only_actual_bare_ref(
) {
    contracts::task54_workflow_tls_v58_real_tls_protocol2_lsrefs_and_create_only_push_change_only_actual_bare_ref();
}
#[test]
fn task54_workflow_edit_v9_tls_unsafe_root_seed_escape_and_unproven_cleanup_preserve_only_owned_evidence(
) {
    contracts::task54_workflow_tls_v58_unsafe_root_and_seed_escape_refuse_before_child_or_config();
}
#[test]
fn task54_workflow_edit_v9_tls_cancel_actual_connect_reaps_owned_openssl_and_joins_io_without_rpc()
{
    contracts::task54_workflow_tls_v58_cancel_actual_connect_reaps_owned_openssl_and_joins_all_io_without_rpc();
}
