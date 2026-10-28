use system2::Noul;
use system2::prelude::*;

trait Rule {
    const TEXT: &'static str;
}

fn rule<R: Rule>() -> Noul {
    noul!(R::TEXT)
}

fn main() {}
