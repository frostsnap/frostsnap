#[test]
fn test_end_to_end() {
    let coordinator = FrostCoordinator::new();
    let devices = (0..3).map(|_| FrostSigner::new());
}
