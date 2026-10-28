mod support;

use std::cell::RefCell;
use support::{
    STATE, answer_set_response, client_for, mock_api, noul_answer, only_request, question_texts,
};
use system2::Noul;
use system2::prelude::*;

const BUGGY: Noul = noul!("Is there a bug in this code?");

const TESTED: Noul = noul!("Is the function tested?", {
    "true": "A test calls the function.",
    "false": "No test calls the function.",
});

const POLICY: Noul = noul!({
    "question": "Does the change obey the policy?",
    "policy": "Credentials never appear in source code.",
});

const VALID_JSON: Noul = noul!(r#"Is {{"a": 1}} valid JSON?"#);

const PURE_TEXT: &str = "Is the function pure?";
const STRICT: bool = true;
const PURE: Noul = noul!(PURE_TEXT);

// the expansion of `noul!` adds no name that can hide a const of the caller, even one that
// another macro names, and no binding that a const with a plain name can match
const __SYSTEM2_TEXT: &str = "Is the name in use?";
#[expect(
    non_upper_case_globals,
    reason = "a binding of that name matches the const"
)]
const text: &str = "Is the text a pattern?";
#[expect(
    non_upper_case_globals,
    reason = "a binding of that name matches the const"
)]
const json: &str = "Is the json a pattern?";

macro_rules! text_from_a_macro {
    () => {
        __SYSTEM2_TEXT
    };
}

#[tokio::test]
async fn noul_sends_each_form_as_written() {
    let function = "parse";
    let rule_text = String::from("Each function has a test.");
    let lines = vec![3, 5];
    let tags = ["parse", "io"];
    let draft = RefCell::new(String::from("Is it done?"));
    let cases = [
        (
            BUGGY,
            r#"{"type":"noul","instructions":"Is there a bug in this code?"}"#,
        ),
        (
            VALID_JSON,
            r#"{"type":"noul","instructions":"Is {\"a\": 1} valid JSON?"}"#,
        ),
        (
            PURE,
            r#"{"type":"noul","instructions":"Is the function pure?"}"#,
        ),
        (
            noul!(text_from_a_macro!()),
            r#"{"type":"noul","instructions":"Is the name in use?"}"#,
        ),
        (
            noul!({
                "draft": draft.borrow().as_str(),
                "old draft": draft.replace(String::from("Is it tested?")),
            }),
            r#"{"type":"noul","instructions":{"draft":"Is it done?","old draft":"Is it done?"}}"#,
        ),
        (
            noul!(text),
            r#"{"type":"noul","instructions":"Is the text a pattern?"}"#,
        ),
        (
            noul!(json),
            r#"{"type":"noul","instructions":"Is the json a pattern?"}"#,
        ),
        (
            noul!(concat!("Is `{", "}` a block?")),
            r#"{"type":"noul","instructions":"Is `{}` a block?"}"#,
        ),
        (
            noul!(if STRICT {
                "Is the code strict?"
            } else {
                "Is the code loose?"
            }),
            r#"{"type":"noul","instructions":"Is the code strict?"}"#,
        ),
        (
            noul!({ "question": "Are these numbers?", "numbers": [01.5, 007, -00.5] }),
            r#"{"type":"noul","instructions":{"question":"Are these numbers?","numbers":[1.5,7,-0.5]}}"#,
        ),
        (
            noul!("Is `{{}}` in `{function}`?"),
            r#"{"type":"noul","instructions":"Is `{}` in `parse`?"}"#,
        ),
        (
            noul!("Is `{function}` tested?"),
            r#"{"type":"noul","instructions":"Is `parse` tested?"}"#,
        ),
        (
            POLICY,
            r#"{"type":"noul","instructions":{"question":"Does the change obey the policy?","policy":"Credentials never appear in source code."}}"#,
        ),
        (
            noul!({ "question": "Does the code obey the rule?", "rule": rule_text }),
            r#"{"type":"noul","instructions":{"question":"Does the code obey the rule?","rule":"Each function has a test."}}"#,
        ),
        (
            noul!([
                "Are these lines tested?",
                { "lines": lines, "tags": tags, "in": "`{function}`, not `{{}}`" },
                [null, true, -1, 2.5, 1., 1e3, "a\"b\\c\nd", "{{}}"],
            ]),
            r#"{"type":"noul","instructions":["Are these lines tested?",{"lines":[3,5],"tags":["parse","io"],"in":"`parse`, not `{}`"},[null,true,-1,2.5,1.0,1000.0,"a\"b\\c\u000ad","{}"]]}"#,
        ),
        (
            TESTED,
            r#"{"type":"noul","instructions":"Is the function tested?","criteria":{"true":"A test calls the function.","false":"No test calls the function."}}"#,
        ),
        (
            noul!("Is it spam?", { "false": "It is a reply.", "true": PURE_TEXT }),
            r#"{"type":"noul","instructions":"Is it spam?","criteria":{"true":"Is the function pure?","false":"It is a reply."}}"#,
        ),
        (
            noul!("Is it spam?", { "true": { "sells": function } }),
            r#"{"type":"noul","instructions":"Is it spam?","criteria":{"true":{"sells":"parse"}}}"#,
        ),
        (
            noul!("Is it spam?", { "false": "It names `{function}`." },),
            r#"{"type":"noul","instructions":"Is it spam?","criteria":{"false":"It names `parse`."}}"#,
        ),
    ];
    let (nouls, texts): (Vec<_>, Vec<_>) = cases.into_iter().unzip();
    let answers: Vec<_> = texts.iter().map(|_| noul_answer(0.5)).collect();
    let server = mock_api(answer_set_response(&answers.iter().collect::<Vec<_>>())).await;

    client_for(&server).ask(nouls, STATE).await.unwrap();

    assert_eq!(question_texts(&only_request(&server).await), texts);
}
