mod support;

use serde_json::json;
use support::{
    INSTRUCTIONS, STATE, answer_set_response, client_for, mock_api, noul, noul_answer,
    only_request, request_text,
};
use system2::prelude::*;
use system2::{Choice, Noul, Score, Tagged};

// no traits at all, so the tag can only come back moved
struct Ticket {
    id: u32,
}

// a `const fn` calls only a `const fn`, so a constant can hold these
const fn tag_noul(noul: Noul) -> Tagged<Noul, &'static str> {
    noul.tag("spam")
}

const fn tag_choice(choice: Choice<&'static str>) -> Tagged<Choice<&'static str>, &'static str> {
    choice.tag("language")
}

const fn tag_score(score: Score<&'static str>) -> Tagged<Score<&'static str>, &'static str> {
    score.tag("size")
}

#[tokio::test]
async fn ask_gives_each_answer_its_tag() {
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
        &noul_answer(0.1),
        &noul_answer(0.2),
    ]))
    .await;
    let spam = tag_noul(noul(INSTRUCTIONS));
    let language = tag_choice(Choice::new("Which language?", ["rust", "go"]).unwrap());
    let size = tag_score(Score::new("How big?", ["A line.", "A module."]).unwrap());
    let ticket = Ticket { id: 7 };
    let replies = (noul("Is it a reply?"), noul("Is it rude?")).tag(ticket);

    let answer_set = client_for(&server)
        .ask((spam, language, size, replies), STATE)
        .await
        .unwrap();

    let ((spam, spam_tag), (language, language_tag), (size, size_tag), replies) = answer_set;
    let ((reply, rude), ticket) = replies;
    assert_eq!((spam.probability(), spam_tag), (0.92, "spam"));
    assert_eq!((*language.most_likely(), language_tag), ("go", "language"));
    assert_eq!((*size.most_likely(), size_tag), ("A line.", "size"));
    assert_eq!((reply.probability(), rude.probability()), (0.1, 0.2));
    assert_eq!(ticket.id, 7);
    assert_eq!(
        request_text(&only_request(&server).await),
        concat!(
            r#"{"model":"jev-latest","state":"Buy now!","questions":{"#,
            r#""q0":{"type":"noul","instructions":"Is this message spam?"},"#,
            r#""q1":{"type":"choice","instructions":"Which language?","criteria":{"rust":null,"go":null}},"#,
            r#""q2":{"type":"score","instructions":"How big?","criteria":["A line.","A module."]},"#,
            r#""q3":{"type":"noul","instructions":"Is it a reply?"},"#,
            r#""q4":{"type":"noul","instructions":"Is it rude?"}}}"#,
        )
    );
}

#[tokio::test]
async fn ask_gives_back_borrowed_tags() {
    let server = mock_api(answer_set_response(&[&noul_answer(0.1), &noul_answer(0.9)])).await;
    let texts = [String::from("spam"), String::from("urgent")];

    let question_set: Vec<_> = texts
        .iter()
        .map(|text| noul(&format!("Is it {text}?")).tag(text))
        .collect();
    let checks = client_for(&server).ask(question_set, STATE).await.unwrap();

    let checks: Vec<_> = checks
        .iter()
        .map(|(likely, text)| (likely.probability(), text.as_str()))
        .collect();
    assert_eq!(checks, [(0.1, "spam"), (0.9, "urgent")]);
}
