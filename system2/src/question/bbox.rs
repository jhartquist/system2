use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed, validate_range};
use crate::json::Json;
use crate::{Image, InvalidAnswer, InvalidJson, QuestionSet, Tagged, json};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

/// A question that finds a region in an image: a bounding box.
///
/// The instructions describe one region of an image in the state. A Bbox needs the model
/// `jev-1.14.0` or a later version. Its answer is a [`BboxAnswer`], with the edges of the region.
/// [`bbox!`](crate::bbox) makes a Bbox that the compiler checks. [`Bbox::new`] makes one from
/// data.
///
/// # Examples
///
/// The smallest Bbox has instructions of text:
///
/// ```
/// use system2::Bbox;
///
/// let dialog = Bbox::new("The error dialog.")?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// The text can come from data:
///
/// ```
/// # use system2::Bbox;
/// let button = "Retry".to_string();
/// let found = Bbox::new(format!("The button with the text `{button}`."))?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// [`bbox!`](crate::bbox) takes the same forms, and the compiler checks them. A question with no
/// variable can be a constant:
///
/// ```
/// use system2::prelude::*;
/// # use system2::Bbox;
///
/// const DIALOG: Bbox = bbox!("The error dialog.");
///
/// let button = "Retry";
/// let found = bbox!("The button with the text `{button}`.");
/// ```
///
/// The state holds the image. The answer gives the region, and `to_pixels` gives it in the pixels
/// of the image:
///
/// ```no_run
/// use system2::prelude::*;
/// use system2::{Image, PixelBox};
///
/// # async fn run(bytes: Vec<u8>) -> system2::Result<()> {
/// let screenshot = Image::new(bytes)?;
/// let dialog = bbox!("The error dialog.").ask(&screenshot).await?;
/// let PixelBox { left, top, .. } = dialog.to_pixels(&screenshot);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Bbox {
    instructions: Json,
}

impl Bbox {
    /// Makes a Bbox from instructions that come from data.
    ///
    /// The instructions are any `Serialize` value that serializes to text, an object, or an
    /// array.
    ///
    /// # Errors
    ///
    /// [`InvalidBbox::Instructions`] if the instructions do not serialize, are empty, or are a
    /// bare number or `bool`.
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidBbox> {
        let instructions = json::validate(instructions).map_err(InvalidBbox::Instructions)?;
        let instructions = Json::from(instructions);
        Ok(Bbox { instructions })
    }

    /// Puts a tag next to the question, so that the answer comes back next to it.
    ///
    /// It is the same as [`Ask::tag`](crate::Ask::tag), but it is a `const fn`, so a constant can
    /// hold a tagged question.
    pub const fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }

    /// Builds the answer that a response with these edges and this confidence would give.
    ///
    /// Each edge is a fraction of the image, from 0 to 1, from the top left. It's for tests of
    /// your own decision code, and it goes through the same checks as a response from the API.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidAnswer`] if a value is not from 0 to 1, or if the left edge is right of
    /// the right edge, or the top edge below the bottom edge.
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

/// An error of [`Bbox::new`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidBbox {
    /// The instructions are not valid.
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
}

/// The answer to a [`Bbox`]: the edges of the region, from 0 to 1.
///
/// Each edge is a fraction of the width or the height of the image, as the API gives it. The API
/// rounds each edge to 0.01. It serializes as
/// `{"left": l, "top": t, "right": r, "bottom": b, "confidence": c}`.
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
    /// Returns the question of the answer.
    #[must_use]
    pub fn question(&self) -> &Bbox {
        &self.question
    }

    /// Returns the question of the answer, with its values, so that code can ask it again.
    #[must_use]
    pub fn into_question(self) -> Bbox {
        self.question
    }

    /// Returns the left edge, from 0 to 1.
    #[must_use]
    pub fn left(&self) -> f64 {
        self.left
    }

    /// Returns the top edge, from 0 to 1.
    #[must_use]
    pub fn top(&self) -> f64 {
        self.top
    }

    /// Returns the right edge, from 0 to 1.
    #[must_use]
    pub fn right(&self) -> f64 {
        self.right
    }

    /// Returns the bottom edge, from 0 to 1.
    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.bottom
    }

    /// Returns the confidence of the API in the region, from 0 to 1.
    #[must_use]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    /// Returns the region in the pixels of the original of `image`.
    ///
    /// `image` must be the image that the API saw. The original is the image that the caller
    /// gave. Thus the region is correct also for a crop, and for an image that the `image` feature
    /// scaled. Each edge rounds to the nearest pixel. `right` and `bottom` are the first pixels
    /// after the region.
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

/// A region of an image, in pixels.
///
/// `right` and `bottom` are the first pixels after the region, so the width is `right - left`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelBox {
    /// The first column of the region.
    pub left: u32,
    /// The first row of the region.
    pub top: u32,
    /// The first column after the region.
    pub right: u32,
    /// The first row after the region.
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
