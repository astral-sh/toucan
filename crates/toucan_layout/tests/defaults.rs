mod fork {
    use toucan_layout as engine;
    include!("fixtures/inputs.rs");
}
mod upstream {
    use upstream_repc as engine;
    include!("fixtures/inputs.rs");
}

#[test]
fn every_target_default_matches_repc_0_1_1() {
    let ours = fork::inputs();
    let original = upstream::inputs();
    assert_eq!(toucan_layout::TARGETS.len(), upstream_repc::TARGETS.len());
    for &target in toucan_layout::TARGETS {
        let previous = *upstream_repc::TARGETS
            .iter()
            .find(|previous| previous.name() == target.name())
            .unwrap();
        for (index, (ours, original)) in ours.iter().zip(&original).enumerate() {
            let expected = upstream_repc::compute_layout(previous, original);
            let actual = toucan_layout::compute_layout(target, ours);
            assert_eq!(
                format!("{actual:?}"),
                format!("{expected:?}"),
                "{} case {index}",
                target.name()
            );
            assert_eq!(
                format!(
                    "{:?}",
                    toucan_layout::compute_layout_with_compiler(
                        target,
                        toucan_layout::system_compiler(target),
                        ours
                    )
                ),
                format!("{expected:?}")
            );
        }
    }
}
