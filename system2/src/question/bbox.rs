use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed, validate_range};
use crate::json::Json;
use crate::{Image, InvalidAnswer, InvalidJson, QuestionSet, Tagged, json};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct Bbox {
    instructions: Json,
}

impl Bbox {
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidBbox> {
        let instructions = json::validate(instructions).map_err(InvalidBbox::Instructions)?;
        let instructions = Json::from(instructions);
        Ok(Bbox { instructions })
    }

    pub const fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }

    pub fn answer(
        self,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
        confidence: f64,
    ) -> Result<BboxAnswer, InvalidAnswer> {
        self.validated_answer(left, top, right, bottom, confidence)
            .map_err(InvalidAnswer::new)
    }

    fn validated_answer(
        self,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
        confidence: f64,
    ) -> Result<BboxAnswer, String> {
        validate_range("left edge", left, 1.0)?;
        validate_range("top edge", top, 1.0)?;
        validate_range("right edge", right, 1.0)?;
        validate_range("bottom edge", bottom, 1.0)?;
        validate_range("confidence", confidence, 1.0)?;
        validate_order("left", left, "right", right)?;
        validate_order("top", top, "bottom", bottom)?;
        let question = self;
        Ok(BboxAnswer {
            question,
            left,
            top,
            right,
            bottom,
            confidence,
        })
    }
}

#[doc(hidden)]
#[must_use]
pub const fn bbox(instructions: Json) -> Bbox {
    Bbox { instructions }
}

impl QuestionSet for Bbox {
    type AnswerSet = BboxAnswer;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidBbox {
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
}

#[derive(Debug, Clone)]
pub struct BboxAnswer {
    question: Bbox,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    confidence: f64,
}

impl BboxAnswer {
    #[must_use]
    pub fn question(&self) -> &Bbox {
        &self.question
    }

    #[must_use]
    pub fn into_question(self) -> Bbox {
        self.question
    }

    #[must_use]
    pub fn left(&self) -> f64 {
        self.left
    }

    #[must_use]
    pub fn top(&self) -> f64 {
        self.top
    }

    #[must_use]
    pub fn right(&self) -> f64 {
        self.right
    }

    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.bottom
    }

    #[must_use]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    #[must_use]
    pub fn to_pixels(&self, image: &Image) -> PixelBox {
        self.pixels_in(image.place())
    }

    // the place of the image in its original, so that a box on a crop maps onto the original
    fn pixels_in(&self, place: PixelBox) -> PixelBox {
        let width = place.right - place.left;
        let height = place.bottom - place.top;
        let left = place.left + to_pixel(self.left, width);
        let top = place.top + to_pixel(self.top, height);
        let right = place.left + to_pixel(self.right, width);
        let bottom = place.top + to_pixel(self.bottom, height);
        PixelBox {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`read` keeps each edge from 0 to 1, so the pixel is from 0 to the size"
)]
fn to_pixel(edge: f64, size: u32) -> u32 {
    (edge * f64::from(size)).round() as u32
}

impl Serialize for BboxAnswer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut answer = serializer.serialize_struct("BboxAnswer", 5)?;
        answer.serialize_field("left", &self.left)?;
        answer.serialize_field("top", &self.top)?;
        answer.serialize_field("right", &self.right)?;
        answer.serialize_field("bottom", &self.bottom)?;
        answer.serialize_field("confidence", &self.confidence)?;
        answer.end()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelBox {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "box")]
struct Question<'a> {
    instructions: &'a Json,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Answer {
    #[serde(rename = "box")]
    Bbox { r#box: Edges, confidence: f64 },
}

#[derive(Deserialize)]
struct Edges {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Sealed for Bbox {
    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        let instructions = &self.instructions;
        raw_question_set.push_bbox(&Question { instructions });
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let Answer::Bbox {
            r#box:
                Edges {
                    left,
                    top,
                    right,
                    bottom,
                },
            confidence,
        } = raw_answer_set.next()?;
        self.validated_answer(left, top, right, bottom, confidence)
    }
}

fn validate_order(start_name: &str, start: f64, end_name: &str, end: f64) -> Result<(), String> {
    if start <= end {
        Ok(())
    } else {
        Err(format!(
            "must give a {start_name} edge at most its {end_name} edge, not {start} and {end}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Bbox, BboxAnswer, InvalidBbox, PixelBox};
    use crate::InvalidJson;
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use serde_json::{Value, json};

    fn bbox_answer(left: f64, top: f64, right: f64, bottom: f64, confidence: f64) -> Value {
        json!({
            "type": "box",
            "box": { "top": top, "bottom": bottom, "left": left, "right": right },
            "confidence": confidence,
        })
    }

    fn pixel_box(left: u32, top: u32, right: u32, bottom: u32) -> PixelBox {
        PixelBox {
            left,
            top,
            right,
            bottom,
        }
    }

    fn serialized(mut bbox: Bbox) -> String {
        let mut raw_question_set = RawQuestionSet::default();
        bbox.write(&mut raw_question_set);
        raw_question_set.texts()[0].to_owned()
    }

    fn read(answer: Value) -> Result<BboxAnswer, String> {
        let bbox = Bbox::new("The mug.").unwrap();
        bbox.read(&mut RawAnswerSet::of(vec![answer]))
    }

    #[test]
    fn new_refuses_invalid_instructions() {
        let cases = [
            (json!(""), "the instructions must not be empty"),
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
            let error = Bbox::new(&instructions).unwrap_err();
            assert!(
                matches!(error, InvalidBbox::Instructions(_)),
                "{instructions}"
            );
            assert_eq!(error.to_string(), message);
        }
        assert!(matches!(
            Bbox::new("").unwrap_err(),
            InvalidBbox::Instructions(InvalidJson::Empty)
        ));
    }

    #[test]
    fn write_gives_the_type_and_the_instructions_and_marks_the_set() {
        let mut bbox = Bbox::new(json!({ "find": "the mug", "near": "the keyboard" })).unwrap();
        let mut raw_question_set = RawQuestionSet::default();

        bbox.write(&mut raw_question_set);

        assert_eq!(
            raw_question_set.texts(),
            [r#"{"type":"box","instructions":{"find":"the mug","near":"the keyboard"}}"#]
        );
        assert!(raw_question_set.has_bbox());
    }

    #[test]
    fn read_takes_the_edges_and_keeps_the_question() {
        let answer = read(bbox_answer(0.2, 0.1, 0.6, 0.5, 0.9)).unwrap();

        let edges = (answer.left(), answer.top(), answer.right(), answer.bottom());
        assert_eq!(edges, (0.2, 0.1, 0.6, 0.5));
        assert_eq!(answer.confidence(), 0.9);
        let question = r#"{"type":"box","instructions":"The mug."}"#;
        assert_eq!(serialized(answer.question().clone()), question);
        assert_eq!(serialized(answer.into_question()), question);
    }

    #[test]
    fn read_takes_a_box_with_no_size() {
        let answer = read(bbox_answer(0.3, 0.4, 0.3, 0.4, 0.1)).unwrap();

        assert_eq!((answer.left(), answer.right()), (0.3, 0.3));
        assert_eq!((answer.top(), answer.bottom()), (0.4, 0.4));
    }

    #[test]
    fn read_refuses_an_answer_that_does_not_fit() {
        let cases = [
            (
                json!({ "type": "noul", "noul": 0.5 }),
                "must answer each question: unknown variant `noul`, expected `box`",
            ),
            (
                json!({ "type": "box", "box": { "top": 0.1, "bottom": 0.5, "left": 0.2 }, "confidence": 0.9 }),
                "must answer each question: missing field `right`",
            ),
            (
                bbox_answer(-0.01, 0.1, 0.6, 0.5, 0.9),
                "must give a left edge from 0 to 1, not -0.01",
            ),
            (
                bbox_answer(0.2, -0.01, 0.6, 0.5, 0.9),
                "must give a top edge from 0 to 1, not -0.01",
            ),
            (
                bbox_answer(0.2, 0.1, 1.01, 0.5, 0.9),
                "must give a right edge from 0 to 1, not 1.01",
            ),
            (
                bbox_answer(0.2, 0.1, 0.6, 1.01, 0.9),
                "must give a bottom edge from 0 to 1, not 1.01",
            ),
            (
                bbox_answer(0.2, 0.1, 0.6, 0.5, 1.01),
                "must give a confidence from 0 to 1, not 1.01",
            ),
            (
                bbox_answer(0.7, 0.1, 0.6, 0.5, 0.9),
                "must give a left edge at most its right edge, not 0.7 and 0.6",
            ),
            (
                bbox_answer(0.2, 0.6, 0.6, 0.5, 0.9),
                "must give a top edge at most its bottom edge, not 0.6 and 0.5",
            ),
        ];
        for (answer, message) in cases {
            assert_eq!(read(answer.clone()).unwrap_err(), message, "{answer}");
        }
    }

    #[test]
    fn pixels_in_rounds_each_edge_and_moves_it_into_the_place() {
        let cases = [
            (
                bbox_answer(0.21, 0.29, 0.61, 0.69, 0.9),
                pixel_box(0, 0, 10, 10),
                pixel_box(2, 3, 6, 7),
            ),
            // 0.29 * 100 is 28.999999999999996, and 0.07 * 100 is 7.000000000000001
            (
                bbox_answer(0.07, 0.29, 0.29, 0.57, 0.9),
                pixel_box(0, 0, 100, 100),
                pixel_box(7, 29, 29, 57),
            ),
            (
                bbox_answer(0.0, 0.0, 1.0, 1.0, 0.9),
                pixel_box(0, 0, 1568, 280),
                pixel_box(0, 0, 1568, 280),
            ),
            (
                bbox_answer(0.25, 0.1, 0.75, 0.5, 0.9),
                pixel_box(1000, 200, 1400, 600),
                pixel_box(1100, 240, 1300, 400),
            ),
        ];
        for (answer, place, pixel_box) in cases {
            assert_eq!(
                read(answer.clone()).unwrap().pixels_in(place),
                pixel_box,
                "{answer}"
            );
        }
    }

    #[test]
    fn serialize_gives_the_edges_and_the_confidence() {
        let answer = read(bbox_answer(0.2, 0.1, 0.6, 0.5, 0.9)).unwrap();

        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            r#"{"left":0.2,"top":0.1,"right":0.6,"bottom":0.5,"confidence":0.9}"#
        );
    }
}
