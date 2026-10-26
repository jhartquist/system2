mod support;

use support::{
    STATE, answer_set_response, client_for, mock_api, noul_answer, only_request, request_body,
};
use system2::prelude::*;
use system2::{Noul, NoulAnswer};

#[tokio::test]
async fn ask_sends_the_questions_of_an_iterator_in_one_request() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)])).await;
    client_for(&server).set_global();
    let texts = ["spam", "urgent"];

    let answers = texts
        .iter()
        .map(|text| Noul::new(format!("Is it {text}?")).unwrap())
        .ask(STATE)
        .await
        .unwrap();

    let probabilities: Vec<_> = answers.iter().map(NoulAnswer::probability).collect();
    assert_eq!(probabilities, [0.1, 0.9]);
    let body = request_body(&only_request(&server).await);
    assert_eq!(body["questions"]["q0"]["instructions"], "Is it spam?");
    assert_eq!(body["questions"]["q1"]["instructions"], "Is it urgent?");
}
