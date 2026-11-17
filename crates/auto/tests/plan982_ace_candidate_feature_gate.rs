//! PLAN-982 A slice — visible gate notice.
//!
//! The permanent candidate cases live in `plan982_ace_candidate_config.rs`
//! behind `#![cfg(feature = "ace-candidate")]`. Without the feature that file
//! compiles to zero tests, which would look like a silent green. This file
//! makes the situation explicit: with the feature OFF it is the only test that
//! runs and it prints why nothing was verified; with the feature ON it is
//! empty and the real cases run.

#![cfg(not(feature = "ace-candidate"))]

#[test]
fn p982_a_candidate_verification_needs_the_feature() {
    eprintln!(
        "PLAN-982 A slice NOT verified by this run: feature `ace-candidate` is off. \
         Run `cargo test -p auto --features ace-candidate --test plan982_ace_candidate_config`."
    );
}
