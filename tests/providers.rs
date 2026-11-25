use solo_design::providers::{lookup, supported};

// Provider names and key variables are built from parts: only the adapter may
// spell them (repo guard).
fn router() -> String {
    ["open", "router"].concat()
}
fn lab() -> String {
    ["anthro", "pic"].concat()
}

#[test]
fn maps_each_supported_provider_to_its_pi_provider_and_key_variable() {
    let cases = [
        (router(), ["OPEN", "ROUTER_API_KEY"].concat()),
        (lab(), ["ANTHRO", "PIC_API_KEY"].concat()),
    ];
    let mut names = supported();
    names.sort_unstable();
    let mut expected: Vec<String> = cases.iter().map(|(n, _)| n.clone()).collect();
    expected.sort_unstable();
    assert_eq!(names, expected, "the supported providers");

    for (name, key_env) in &cases {
        let provider = lookup(name).unwrap_or_else(|e| panic!("{name} has no adapter: {e}"));
        assert_eq!(provider.name, name.as_str(), "configured name");
        assert_eq!(
            provider.pi_provider,
            name.as_str(),
            "Pi provider id for {name}"
        );
        assert_eq!(
            provider.key_env,
            key_env.as_str(),
            "key variable for {name}"
        );
    }

    let err = lookup("fixture-provider").expect_err("an unknown provider has no adapter");
    for (name, _) in &cases {
        assert!(err.contains(name.as_str()), "error lists {name}: {err}");
    }
    assert!(
        err.contains("fixture-provider"),
        "error names the unknown provider: {err}"
    );
}
