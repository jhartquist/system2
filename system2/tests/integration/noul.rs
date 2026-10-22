use serde_json::json;
use std::collections::BTreeMap;
use system2::{InvalidJson, InvalidNoul, Noul};

#[test]
fn new_takes_text_an_object_or_an_array() {
    assert!(Noul::new("Is it spam?").is_ok());
    assert!(Noul::new(json!({ "question": "Is it spam?", "policy": "No ads." })).is_ok());
    assert!(Noul::new(["Is it spam?", "Answer for the whole thread."]).is_ok());
}

#[test]
fn new_refuses_instructions_that_the_api_refuses() {
    let cases = [
        (json!(""), "the instructions must not be empty"),
        (json!(" \n\t"), "the instructions must not be empty"),
        (json!({}), "the instructions must not be empty"),
        (json!([]), "the instructions must not be empty"),
        (json!(null), "the instructions must not be empty"),
        (
            json!(42),
            "the instructions must be text, an object, or an array",
        ),
        (
            json!(true),
            "the instructions must be text, an object, or an array",
        ),
    ];
    for (instructions, message) in cases {
        assert_eq!(
            Noul::new(instructions.clone()).unwrap_err().to_string(),
            message,
            "{instructions}"
        );
    }
}

#[test]
fn new_refuses_instructions_that_do_not_serialize() {
    let pairs = BTreeMap::from([((1, 2), "a pair as a key")]);

    assert_eq!(
        Noul::new(pairs).unwrap_err().to_string(),
        "the instructions must serialize to JSON: key must be a string"
    );
}

#[test]
fn with_criteria_takes_true_false_or_both() {
    let cases = [
        json!({ "true": "It sells a product." }),
        json!({ "false": { "covers": "questions and replies" } }),
        json!({ "false": "It is a reply.", "true": "It sells a product." }),
    ];
    for criteria in cases {
        assert!(Noul::with_criteria("Is it spam?", criteria).is_ok());
    }
}

#[test]
fn with_criteria_refuses_criteria_that_the_api_refuses() {
    let cases = [
        (
            json!("It sells a product."),
            "the criteria must be an object with the key `true`, the key `false`, or both",
        ),
        (
            json!({}),
            "the criteria must be an object with the key `true`, the key `false`, or both",
        ),
        (
            json!({ "ture": "It sells a product." }),
            "the criteria must be an object with the key `true`, the key `false`, or both",
        ),
        (
            json!({ "true": "It sells a product.", "maybe": "It links out." }),
            "the criteria must be an object with the key `true`, the key `false`, or both",
        ),
        (
            json!({ "true": "" }),
            "the `true` criterion must not be empty",
        ),
        (
            json!({ "true": null }),
            "the `true` criterion must not be empty",
        ),
        (
            json!({ "false": 0 }),
            "the `false` criterion must be text, an object, or an array",
        ),
    ];
    for (criteria, message) in cases {
        let error = Noul::with_criteria("Is it spam?", criteria.clone()).unwrap_err();
        assert_eq!(error.to_string(), message, "{criteria}");
    }
}

#[test]
fn with_criteria_refuses_criteria_that_do_not_serialize() {
    let pairs = BTreeMap::from([((1, 2), "a pair as a key")]);

    assert_eq!(
        Noul::with_criteria("Is it spam?", pairs)
            .unwrap_err()
            .to_string(),
        "the criteria must be an object with the key `true`, the key `false`, or both"
    );
}

#[test]
fn with_criteria_refuses_instructions_that_the_api_refuses() {
    let error = Noul::with_criteria("", json!({ "true": "It sells a product." })).unwrap_err();

    assert_eq!(error.to_string(), "the instructions must not be empty");
}

#[test]
fn an_error_names_the_part_and_the_problem() {
    assert!(matches!(
        Noul::new(""),
        Err(InvalidNoul::Instructions(InvalidJson::Empty))
    ));
    assert!(matches!(
        Noul::with_criteria("Is it spam?", json!({ "false": 0 })),
        Err(InvalidNoul::False(InvalidJson::Shape))
    ));
}
