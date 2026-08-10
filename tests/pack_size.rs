//! Reading a pack label as an amount. No DB: this is one pure function, and
//! what is worth pinning is which labels it reads, which it refuses, and that
//! it never invents a dimension for one it does not understand.
//!
//! The labels here are the shapes the live catalogue actually holds — a shop's
//! own text, spelled every way the shops spell it.

use life::products::packsize::{PackSize, PackUnit, parse};

fn grams(value: f64) -> Option<PackSize> {
    Some(PackSize {
        value,
        unit: PackUnit::Gram,
    })
}

fn millilitres(value: f64) -> Option<PackSize> {
    Some(PackSize {
        value,
        unit: PackUnit::Millilitre,
    })
}

#[test]
fn a_symbol_reads_with_or_without_a_space() {
    // Both spellings are in the catalogue, of the same amount of the same thing.
    assert_eq!(parse("100g"), grams(100.0));
    assert_eq!(parse("100 g"), grams(100.0));
}

#[test]
fn case_never_matters() {
    // Asda shouts its units and Open Food Facts does not.
    assert_eq!(parse("250ML"), millilitres(250.0));
    assert_eq!(parse("1L"), millilitres(1000.0));
}

#[test]
fn a_unit_spelled_out_reads_the_same_as_its_symbol() {
    assert_eq!(parse("35 gm"), grams(35.0));
    assert_eq!(parse("35 grammes"), grams(35.0));
    assert_eq!(parse("750 grams"), grams(750.0));
}

#[test]
fn everything_mass_lands_in_grams_and_everything_volume_in_millilitres() {
    // The whole point of canonicalising: after this nobody downstream holds a
    // conversion table, because there is nothing left to convert.
    assert_eq!(parse("1.5kg"), grams(1500.0));
    assert_eq!(parse("500mg"), grams(0.5));
    assert_eq!(parse("33 cl"), millilitres(330.0));
    assert_eq!(parse("50 cl"), millilitres(500.0));
    assert_eq!(parse("2 litres"), millilitres(2000.0));
}

#[test]
fn a_multipack_reads_as_what_is_in_the_cupboard() {
    // 22 sachets of 27g is 594g of oats, and 594g is what you can cook with —
    // the sachet count is on the label for anyone who wants it.
    assert_eq!(parse("22x27G"), grams(594.0));
    assert_eq!(parse("6 x 330ml"), millilitres(1980.0));
    assert_eq!(parse("4 × 125 g"), grams(500.0));
}

#[test]
fn sold_by_the_item_is_one_of_them() {
    assert_eq!(
        parse("EACH"),
        Some(PackSize {
            value: 1.0,
            unit: PackUnit::Count
        })
    );
}

#[test]
fn an_ambiguous_unit_is_refused_rather_than_guessed() {
    // A grocery `oz` is usually mass and sometimes fluid, and the label does
    // not say which. Refusing shows the raw text, which is what happens today;
    // guessing puts a number in the cupboard that nobody measured.
    assert_eq!(parse("16oz"), None);
    assert_eq!(parse("1lb"), None);
    assert_eq!(parse("8 fl oz"), None);
}

#[test]
fn an_abbreviation_we_only_think_we_know_is_refused() {
    // `gr` and `ltr` are almost certainly grams and litres. "Almost certainly"
    // is not the standard for writing a number into stock.
    assert_eq!(parse("100gr"), None);
    assert_eq!(parse("2 ltr"), None);
}

#[test]
fn a_label_that_is_not_an_amount_reads_as_nothing() {
    assert_eq!(parse(""), None);
    assert_eq!(parse("   "), None);
    assert_eq!(parse("family pack"), None);
    // A bare number could be six eggs or a truncated `6 x 330ml`; nothing in
    // the string says which.
    assert_eq!(parse("6"), None);
}

#[test]
fn an_x_inside_a_word_does_not_make_it_a_multipack() {
    // `split_once('x')` on "box of 6" yields "bo", which is not a count — so
    // the label goes down the ordinary path and is refused there, rather than
    // being read as something-of-something.
    assert_eq!(parse("box of 6"), None);
    assert_eq!(parse("6 boxes"), None);
}

#[test]
fn trailing_or_leading_noise_is_refused_not_ignored() {
    // The unit must be all that follows the number. Reading "250g tub" as 250g
    // is right often enough to be tempting and wrong often enough to hurt —
    // "250g drained" is not 250g of what you are about to cook with.
    assert_eq!(parse("250g tub"), None);
    assert_eq!(parse("approx 250g"), None);
}

#[test]
fn a_pack_of_nothing_is_not_a_reading() {
    // Zero is what a malformed label parses to more often than it is a real
    // pack size, and a zero-quantity stock row would read as "we are out".
    assert_eq!(parse("0g"), None);
    assert_eq!(parse("0x100g"), None);
}
