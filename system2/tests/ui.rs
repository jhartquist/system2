#[test]
fn ui_cases_fail_with_their_messages() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
    #[cfg(not(feature = "image"))]
    cases.compile_fail("tests/ui/no_image_feature/*.rs");
    #[cfg(feature = "image")]
    cases.compile_fail("tests/ui/image_feature/*.rs");
}
