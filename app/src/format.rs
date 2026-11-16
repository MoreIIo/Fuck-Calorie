//! Mise en forme des nombres et des dates, à la française (§8.7).

use chrono::{Datelike, NaiveDate, Weekday};

/// Espace insécable, séparateur des milliers. (L'espace fine U+202F serait plus juste
/// typographiquement, mais Urbanist la dessine presque invisible.)
const NBSP: char = '\u{00A0}';
/// Vrai signe moins (plus lisible que le trait d'union).
const MINUS: char = '\u{2212}';

/// Entier arrondi, avec séparateur des milliers : 1 528.
pub fn int(value: f64) -> String {
    let rounded = value.round();
    let digits = format!("{:.0}", rounded.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(NBSP);
        }
        out.push(c);
    }
    if rounded < 0.0 {
        format!("{MINUS}{out}")
    } else {
        out
    }
}

/// Entier signé : +2 100 / −350 / 0.
pub fn signed_int(value: f64) -> String {
    if value.round() > 0.0 {
        format!("+{}", int(value))
    } else {
        int(value)
    }
}

/// Nombre à une décimale avec virgule : 69,9.
pub fn one_decimal(value: f64) -> String {
    let text = format!("{:.1}", value.abs()).replace('.', ",");
    if value < -0.05 {
        format!("{MINUS}{text}")
    } else {
        text
    }
}

/// Quantité saisie ou affichée : deux décimales au plus, sans zéros inutiles (125 ; 0,5 ;
/// 12,25).
pub fn quantity(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0')
        .trim_end_matches('.')
        .replace('.', ",")
}

/// Variation signée à une décimale : +0,3 / −0,4.
pub fn signed_one_decimal(value: f64) -> String {
    if value > 0.05 {
        format!("+{}", one_decimal(value))
    } else {
        one_decimal(value)
    }
}

/// Lit un nombre saisi, avec virgule ou point : « 69,5 » → 69,5. Vide → `None`.
pub fn parse_number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != NBSP && *c != '\u{202F}')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "lundi",
        Weekday::Tue => "mardi",
        Weekday::Wed => "mercredi",
        Weekday::Thu => "jeudi",
        Weekday::Fri => "vendredi",
        Weekday::Sat => "samedi",
        Weekday::Sun => "dimanche",
    }
}

/// « dimanche 15 novembre » (l'année n'est ajoutée que si elle diffère de `today`).
pub fn long_date(date: NaiveDate, today: NaiveDate) -> String {
    let base = format!(
        "{} {} {}",
        weekday(date.weekday()),
        date.day(),
        MONTHS[date.month0() as usize]
    );
    if date.year() == today.year() {
        base
    } else {
        format!("{base} {}", date.year())
    }
}

/// « 8 novembre ».
pub fn day_month(date: NaiveDate) -> String {
    format!("{} {}", date.day(), MONTHS[date.month0() as usize])
}

/// « 7 mars 2027 ».
pub fn short_date(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        date.day(),
        MONTHS[date.month0() as usize],
        date.year()
    )
}

/// Titre de la journée : « Aujourd'hui », « Hier », « Demain » ou la date.
pub fn day_title(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Aujourd'hui".to_owned(),
        -1 => "Hier".to_owned(),
        1 => "Demain".to_owned(),
        _ => {
            let text = long_date(date, today);
            let mut chars = text.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn integers() {
        assert_eq!(int(0.0), "0");
        assert_eq!(int(999.4), "999");
        assert_eq!(int(1528.4), "1\u{00A0}528");
        assert_eq!(int(1_234_567.0), "1\u{00A0}234\u{00A0}567");
        assert_eq!(int(-350.0), "\u{2212}350");
        assert_eq!(int(-0.2), "0");
        assert_eq!(signed_int(2_100.0), "+2\u{00A0}100");
        assert_eq!(signed_int(-350.0), "\u{2212}350");
        assert_eq!(signed_int(0.3), "0");
    }

    #[test]
    fn decimals() {
        assert_eq!(one_decimal(69.94), "69,9");
        assert_eq!(one_decimal(70.0), "70,0");
        assert_eq!(signed_one_decimal(-0.44), "\u{2212}0,4");
        assert_eq!(signed_one_decimal(0.3), "+0,3");
        assert_eq!(signed_one_decimal(0.01), "0,0");
    }

    #[test]
    fn quantities() {
        assert_eq!(quantity(125.0), "125");
        assert_eq!(quantity(0.5), "0,5");
        assert_eq!(quantity(12.25), "12,25");
        assert_eq!(quantity(33.333), "33,33");
        assert_eq!(quantity(100.0), "100");
        // Ce qui est affiché se relit tel quel.
        assert_eq!(parse_number(&quantity(62.5)), Some(62.5));
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_number("69,5"), Some(69.5));
        assert_eq!(parse_number(" 1 200 "), Some(1200.0));
        assert_eq!(parse_number("70.2"), Some(70.2));
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("NaN"), None);
    }

    #[test]
    fn dates() {
        let today = date(2026, 11, 15);
        assert_eq!(long_date(today, today), "dimanche 15 novembre");
        assert_eq!(long_date(date(2025, 8, 1), today), "vendredi 1 août 2025");
        assert_eq!(short_date(date(2027, 3, 7)), "7 mars 2027");
        assert_eq!(day_month(date(2026, 11, 8)), "8 novembre");
        assert_eq!(day_title(today, today), "Aujourd'hui");
        assert_eq!(day_title(date(2026, 11, 14), today), "Hier");
        assert_eq!(day_title(date(2026, 11, 16), today), "Demain");
        assert_eq!(day_title(date(2026, 11, 10), today), "Mardi 10 novembre");
    }
}
