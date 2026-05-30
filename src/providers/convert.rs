use crate::currency::CurrencyCache;
use crate::engine::Provider;
use crate::model::{Action, Item};

/// Unit and currency conversion. Parses natural forms like "10 km in mi",
/// "100 f to c", and "100 usd to eur". Units are offline hand-rolled tables;
/// currency uses cached rates (refreshed off the worker thread).
pub struct ConvertProvider {
    currency: CurrencyCache,
}

impl ConvertProvider {
    pub fn new(currency: CurrencyCache) -> Self {
        Self { currency }
    }
}

impl Provider for ConvertProvider {
    fn name(&self) -> &'static str {
        "convert"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let Some((amount, from, to)) = parse(query) else {
            return;
        };

        // Physical units (length, mass, volume, data, speed, time).
        if let Some(value) = convert_units(amount, &from, &to) {
            push_result(out, value, &to, "Convert - Enter to copy");
            return;
        }
        // Temperature is affine, handled separately.
        if let Some(value) = convert_temperature(amount, &from, &to) {
            push_result(out, value, &to, "Convert - Enter to copy");
            return;
        }
        // Currency: only when both tokens look like ISO codes.
        if is_currency_code(&from) && is_currency_code(&to) {
            if self.currency.is_stale() {
                self.currency.refresh_async();
            }
            let fu = from.to_ascii_uppercase();
            let tu = to.to_ascii_uppercase();
            match self.currency.convert(amount, &fu, &tu) {
                Some((value, date)) => {
                    let subtitle = if date.is_empty() {
                        "Currency - Enter to copy".to_string()
                    } else {
                        format!("Rates from {date} - Enter to copy")
                    };
                    push_currency(out, value, &tu, &subtitle);
                }
                None if !self.currency.has_rates() => {
                    out.push(Item::new(
                        "Currency rates unavailable offline",
                        "Connect to the internet and try again",
                        "Convert",
                        9_500,
                        Action::None,
                    ));
                }
                None => {}
            }
        }
    }
}

fn push_result(out: &mut Vec<Item>, value: f64, unit: &str, subtitle: &str) {
    let formatted = format!("{} {unit}", format_number(value));
    out.push(Item::new(
        format!("= {formatted}"),
        subtitle,
        "Convert",
        // Just under calc (10_000); an explicit conversion is unambiguous.
        9_500,
        Action::CopyText(formatted),
    ));
}

fn push_currency(out: &mut Vec<Item>, value: f64, code: &str, subtitle: &str) {
    let formatted = format!("{:.2} {code}", value);
    out.push(Item::new(
        format!("= {formatted}"),
        subtitle,
        "Convert",
        9_500,
        Action::CopyText(formatted),
    ));
}

/// Parse "<number> <from> (in|to) <to>" with flexible spacing.
fn parse(query: &str) -> Option<(f64, String, String)> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let lower = q.to_ascii_lowercase();
    let (left, right) = split_separator(&lower)?;
    let to = right.trim();
    if to.is_empty() || !to.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let (amount, from) = parse_amount_unit(left.trim())?;
    if from.is_empty() {
        return None;
    }
    Some((amount, from.to_string(), to.to_string()))
}

/// Split on the first ` in ` or ` to ` separator.
fn split_separator(q: &str) -> Option<(&str, &str)> {
    for sep in [" in ", " to ", " -> "] {
        if let Some(idx) = q.find(sep) {
            return Some((&q[..idx], &q[idx + sep.len()..]));
        }
    }
    None
}

/// Split a leading number from its (possibly space-separated) unit token.
fn parse_amount_unit(s: &str) -> Option<(f64, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let mut seen_digit = false;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        if bytes[i].is_ascii_digit() {
            seen_digit = true;
        }
        i += 1;
    }
    if !seen_digit {
        return None;
    }
    let amount: f64 = s[..i].parse().ok()?;
    let unit = s[i..].trim();
    Some((amount, unit))
}

#[derive(PartialEq)]
enum Dim {
    Length,
    Mass,
    Volume,
    Data,
    Speed,
    Time,
}

/// Convert between two units of the same physical dimension.
fn convert_units(amount: f64, from: &str, to: &str) -> Option<f64> {
    let (from_dim, from_factor) = unit_factor(from)?;
    let (to_dim, to_factor) = unit_factor(to)?;
    if from_dim != to_dim {
        return None;
    }
    Some(amount * from_factor / to_factor)
}

/// Returns the unit's dimension and factor to the dimension's canonical base.
fn unit_factor(unit: &str) -> Option<(Dim, f64)> {
    let f = |d, v| Some((d, v));
    match unit {
        // Length (base: meter)
        "mm" => f(Dim::Length, 0.001),
        "cm" => f(Dim::Length, 0.01),
        "dm" => f(Dim::Length, 0.1),
        "m" | "meter" | "meters" | "metre" | "metres" => f(Dim::Length, 1.0),
        "km" | "kilometer" | "kilometers" => f(Dim::Length, 1000.0),
        "in" | "inch" | "inches" => f(Dim::Length, 0.0254),
        "ft" | "foot" | "feet" => f(Dim::Length, 0.3048),
        "yd" | "yard" | "yards" => f(Dim::Length, 0.9144),
        "mi" | "mile" | "miles" => f(Dim::Length, 1609.344),
        "nmi" => f(Dim::Length, 1852.0),
        // Mass (base: gram)
        "mg" => f(Dim::Mass, 0.001),
        "g" | "gram" | "grams" => f(Dim::Mass, 1.0),
        "kg" | "kilogram" | "kilograms" => f(Dim::Mass, 1000.0),
        "t" | "ton" | "tonne" | "tonnes" => f(Dim::Mass, 1_000_000.0),
        "oz" | "ounce" | "ounces" => f(Dim::Mass, 28.349523125),
        "lb" | "lbs" | "pound" | "pounds" => f(Dim::Mass, 453.59237),
        "st" | "stone" => f(Dim::Mass, 6350.29318),
        // Volume (base: liter)
        "ml" => f(Dim::Volume, 0.001),
        "cl" => f(Dim::Volume, 0.01),
        "dl" => f(Dim::Volume, 0.1),
        "l" | "liter" | "liters" | "litre" | "litres" => f(Dim::Volume, 1.0),
        "tsp" => f(Dim::Volume, 0.00492892),
        "tbsp" => f(Dim::Volume, 0.0147868),
        "floz" => f(Dim::Volume, 0.0295735),
        "cup" | "cups" => f(Dim::Volume, 0.236588),
        "pt" | "pint" | "pints" => f(Dim::Volume, 0.473176),
        "qt" | "quart" | "quarts" => f(Dim::Volume, 0.946353),
        "gal" | "gallon" | "gallons" => f(Dim::Volume, 3.785411784),
        // Data (base: byte)
        "bit" | "bits" => f(Dim::Data, 0.125),
        "b" | "byte" | "bytes" => f(Dim::Data, 1.0),
        "kb" => f(Dim::Data, 1.0e3),
        "mb" => f(Dim::Data, 1.0e6),
        "gb" => f(Dim::Data, 1.0e9),
        "tb" => f(Dim::Data, 1.0e12),
        "kib" => f(Dim::Data, 1024.0),
        "mib" => f(Dim::Data, 1024.0 * 1024.0),
        "gib" => f(Dim::Data, 1024.0 * 1024.0 * 1024.0),
        "tib" => f(Dim::Data, 1024.0 * 1024.0 * 1024.0 * 1024.0),
        // Speed (base: meters/second)
        "mps" => f(Dim::Speed, 1.0),
        "kph" | "kmh" => f(Dim::Speed, 0.2777777778),
        "mph" => f(Dim::Speed, 0.44704),
        "fps" => f(Dim::Speed, 0.3048),
        "knot" | "knots" | "kn" => f(Dim::Speed, 0.5144444444),
        // Time (base: second)
        "s" | "sec" | "secs" | "second" | "seconds" => f(Dim::Time, 1.0),
        "min" | "mins" | "minute" | "minutes" => f(Dim::Time, 60.0),
        "h" | "hr" | "hrs" | "hour" | "hours" => f(Dim::Time, 3600.0),
        "day" | "days" => f(Dim::Time, 86400.0),
        "week" | "weeks" => f(Dim::Time, 604800.0),
        _ => None,
    }
}

/// Temperature is affine, so it gets its own conversion via Celsius.
fn convert_temperature(amount: f64, from: &str, to: &str) -> Option<f64> {
    let celsius = match from {
        "c" | "celsius" => amount,
        "f" | "fahrenheit" => (amount - 32.0) * 5.0 / 9.0,
        "k" | "kelvin" => amount - 273.15,
        _ => return None,
    };
    match to {
        "c" | "celsius" => Some(celsius),
        "f" | "fahrenheit" => Some(celsius * 9.0 / 5.0 + 32.0),
        "k" | "kelvin" => Some(celsius + 273.15),
        _ => None,
    }
}

fn is_currency_code(token: &str) -> bool {
    token.len() == 3 && token.chars().all(|c| c.is_ascii_alphabetic())
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        let s = format!("{value:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
