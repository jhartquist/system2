# citation_check

Checks if eight citations of RFC 7519 are true.
This is a port of the TypeSafe cookbook
[citation_check](https://docs.typesafe.ai/cookbooks/citation_check).

## Run

Set `TYPESAFE_API_KEY`, then run:

```sh
cargo run -p citation_check
```

## Output

```text
citation          verdict         conf  action
----------------------------------------------
epoch_seconds     verified        0.97  auto
aud_reject        verified        0.98  auto
sig_reporting     fabricated         -  auto
clock_skew        verified        1.00  auto
exp_required      contradicted    1.00  auto
pii_encryption    unsupported     0.46  review
iat_future        unsupported     0.97  auto
duplicate_names   verified        1.00  auto
```
