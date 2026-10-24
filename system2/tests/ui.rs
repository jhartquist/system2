#[test]
fn ui_cases_fail_with_their_messages() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
