mod conformance_vectors;

#[test]
fn native_shared_golden_vectors() {
    conformance_vectors::run_shared_vectors();
}
