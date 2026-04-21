// logos-collab/tests/collab_stress.rs
//
//! End-to-end stress integration tests.
//!
//! Run with the `stress` feature:
//!   cargo test -p logos-collab --features stress --test collab_stress
//!
//! These tests are kept separate from unit tests so they can be gated
//! in CI (`--features stress` required).

#![cfg(feature = "stress")]

use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use logos_collab::stress::{
    Report, SharedState, SimDriver, SimUser, Thresholds,
};

// ── I-01: 10 users × 100 ops each — passes with relaxed thresholds ───────────

#[tokio::test]
async fn i_01_ten_users_100_ops_each() {
    let state = Arc::new(Mutex::new(SharedState::new(Uuid::new_v4())));
    let users: Vec<SimUser> = (0..10)
        .map(|_| SimUser::new(Uuid::new_v4(), SimDriver::default_script(100)))
        .collect();

    let metrics = SimDriver::run_local(users, state).await;
    let report  = Report::build(&metrics, Thresholds::relaxed());

    println!("{}", report.render_text());
    assert_eq!(metrics.total_ops, 1_000, "10 users × 100 ops = 1 000 total");
    assert_eq!(metrics.error_count, 0,    "no errors expected");
    assert!(report.passed(), "relaxed thresholds should pass:\n{}", report.render_json());
}

// ── I-02: 50 users × 20 ops each — error rate exactly 0 ─────────────────────

#[tokio::test]
async fn i_02_fifty_users_zero_errors() {
    let state = Arc::new(Mutex::new(SharedState::new(Uuid::new_v4())));
    let users: Vec<SimUser> = (0..50)
        .map(|_| SimUser::new(Uuid::new_v4(), SimDriver::default_script(20)))
        .collect();

    let metrics = SimDriver::run_local(users, state).await;
    assert_eq!(metrics.total_ops,   1_000);
    assert_eq!(metrics.error_count, 0);
    assert!((metrics.error_rate() - 0.0).abs() < f64::EPSILON);
}
