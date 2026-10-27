mod support;

use serde_json::json;
use support::{
    INSTRUCTIONS, STATE, answer_set_response, client_for, mock_api, no_request_sent, noul,
    noul_answer, noul_response, only_request, request_body, request_text,
};
use system2::{AskError, Choice, InvalidJson, Noul, Score};

const REQUEST_ID: &str = "req_01a13d1fbca4728f8f7c1cdd312bd3a5";

#[tokio::test]
async fn ask_sends_a_tuple_in_one_request() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.92),
        &json!({
            "type": "choice",
            "choice": "go",
            "confidence": 0.7,
            "probabilities": { "rust": 0.3, "go": 0.7 },
        }),
        &json!({
            "type": "score",
            "score": 0.2,
            "confidence": 0.8,
            "probabilities": { "0": 0.8, "1": 0.2 },
        }),
    ]))
    .await;
    let question_set = (
        noul(INSTRUCTIONS),
        Choice::new("Which language?", ["rust", "go"]).unwrap(),
        Score::new("How big?", ["A line.", "A module."]).unwrap(),
    );

    let (spam, language, size) = client_for(&server).ask(question_set, STATE).await.unwrap();

    assert_eq!(spam.probability(), 0.92);
    assert_eq!(*language.most_likely(), "go");
    assert_eq!(*size.most_likely(), "A line.");
    assert_eq!(
        request_text(&only_request(&server).await),
        concat!(
            r#"{"model":"jev-latest","state":"Buy now!","questions":{"#,
            r#""q0":{"type":"noul","instructions":"Is this message spam?"},"#,
            r#""q1":{"type":"choice","instructions":"Which language?","criteria":{"rust":null,"go":null}},"#,
            r#""q2":{"type":"score","instructions":"How big?","criteria":["A line.","A module."]}}}"#,
        )
    );
}

#[tokio::test]
async fn ask_sends_nested_question_sets_in_one_request() {
    let server = mock_api(answer_set_response(&[
        &noul_answer(0.1),
        &noul_answer(0.2),
        &noul_answer(0.3),
        &noul_answer(0.4),
    ]))
    .await;
    let question_set = (
        [noul("Is it spam?"), noul("Is it urgent?")],
        vec![noul("Is it a reply?")],
        (Some(noul("Is it rude?")), None::<Noul>),
    );

    let ([spam, urgent], replies, (rude, none)) =
        client_for(&server).ask(question_set, STATE).await.unwrap();

    assert_eq!(spam.probability(), 0.1);
    assert_eq!(urgent.probability(), 0.2);
    assert_eq!(replies[0].probability(), 0.3);
    assert_eq!(rude.unwrap().probability(), 0.4);
    assert!(none.is_none());
    let body = request_body(&only_request(&server).await);
    let instructions: Vec<_> = (0..4)
        .map(|index| body["questions"][format!("q{index}")]["instructions"].clone())
        .collect();
    assert_eq!(
        instructions,
        [
            "Is it spam?",
            "Is it urgent?",
            "Is it a reply?",
            "Is it rude?"
        ]
    );
    assert_eq!(body["questions"].as_object().unwrap().len(), 4);
}

#[tokio::test]
async fn ask_sends_a_vec_in_one_request() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)])).await;
    let rules = ["Is it spam?", "Is it urgent?"];

    let question_set = rules
        .iter()
        .map(Noul::new)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let answer_set = client_for(&server).ask(question_set, STATE).await.unwrap();

    assert_eq!(answer_set[0].probability(), 0.1);
    assert_eq!(answer_set[1].probability(), 0.9);
    let body = request_body(&only_request(&server).await);
    assert_eq!(body["questions"]["q1"]["instructions"], "Is it urgent?");
}

#[tokio::test]
async fn ask_sends_no_request_for_a_question_set_with_no_questions() {
    let server = mock_api(noul_response(0.92)).await;
    let client = client_for(&server);

    let empty = client.ask(Vec::<Noul>::new(), STATE).await.unwrap();
    let none = client.ask(None::<Noul>, STATE).await.unwrap();

    assert!(empty.is_empty());
    assert!(none.is_none());
    assert!(no_request_sent(&server).await);
}

#[tokio::test]
async fn ask_refuses_a_bad_state_for_a_question_set_with_no_questions() {
    let server = mock_api(noul_response(0.92)).await;

    let error = client_for(&server)
        .ask(Vec::<Noul>::new(), "")
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::State(InvalidJson::Empty)));
}

#[tokio::test]
async fn ask_refuses_answers_that_do_not_fit_the_questions() {
    let cases = [
        (
            vec![noul_answer(0.1)],
            "must answer each question: `q1` has no answer",
        ),
        (
            vec![noul_answer(0.1), noul_answer(0.2), noul_answer(0.3)],
            "must answer only the questions of the request, not `q2`",
        ),
        (
            vec![
                noul_answer(0.1),
                json!({
                    "type": "choice",
                    "choice": "go",
                    "confidence": 0.7,
                    "probabilities": { "rust": 0.3, "go": 0.7 },
                }),
            ],
            "must answer each question: unknown variant `choice`, expected `noul`",
        ),
    ];
    for (answers, message) in cases {
        let answers: Vec<_> = answers.iter().collect();
        let response =
            answer_set_response(&answers).insert_header("x-typesafe-request-id", REQUEST_ID);
        let server = mock_api(response).await;

        let error = client_for(&server)
            .ask((noul("Is it spam?"), noul("Is it urgent?")), STATE)
            .await
            .unwrap_err();

        assert!(matches!(error, AskError::InvalidResponse { .. }));
        assert_eq!(
            error.to_string(),
            format!("the response {message} (request ID `{REQUEST_ID}`)")
        );
    }
}
