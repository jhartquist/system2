# find_in_image

Finds an object in a photo. Then writes a crop of the object, or the photo with a rectangle
around the object. One request asks if the photo shows the object, and where. The output has the
size of the original photo.

## Run

Set `TYPESAFE_API_KEY`, then run:

```sh
cargo run -p find_in_image --features image -- \
    examples/find_in_image/data/desk.jpg "the coffee cup" draw cup.jpg
```

Write `crop` in place of `draw` to get only the object.

## Output

```text
found: the coffee cup at (1229, 306) to (1440, 549) (probability 0.94, confidence 0.91)
```

![The photo, with a rectangle around the coffee cup](cup.jpg)

An object that is not in the photo gives no file:

```text
not found: a cat (probability 0.05)
```
