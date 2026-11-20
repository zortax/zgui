//! What is left out of one line of a drawing, or written in a form both windows agree on.

/// Returns `line` with its paint space and its mark bin left out, and with its atlas tile split
/// off: the line without the tile, and the tile's address.
pub(super) fn canonical(line: &str) -> (String, Option<String>) {
    split_tile(&without_brush(&without_bin(&without_solid_origin(line))))
}

/// How the transcript spells an atlas tile.
const TILE: &str = " tile=page";

/// Returns `line` without its atlas tile, and the tile's address.
fn split_tile(line: &str) -> (String, Option<String>) {
    let Some(at) = line.find(TILE) else {
        return (line.to_owned(), None);
    };
    let rest = &line[at + " tile=".len()..];
    let end = rest.find(']').map_or(rest.len(), |close| close + 1);
    (
        format!("{}{}", &line[..at], &rest[end..]),
        Some(rest[..end].to_owned()),
    )
}

/// Returns `line` without the bin the mark plan put a marks instance in.
fn without_bin(line: &str) -> String {
    match line
        .find(" bin=page")
        .or_else(|| line.find(" overflow_bin "))
    {
        Some(at) => line[..at].to_owned(),
        None => line.to_owned(),
    }
}

/// Reports whether every paint `line` names is solid or none.
fn solid(line: &str) -> bool {
    ["fill=", "stroke=", "paint="].iter().all(|field| {
        line.split(' ')
            .filter_map(|word| word.strip_prefix(field))
            .all(|paint| matches!(paint, "solid" | "none"))
    })
}

/// Returns `line` without its paint origin when every paint it names is solid or none.
fn without_solid_origin(line: &str) -> String {
    let Some(at) = line.find(" paint_origin=[") else {
        return line.to_owned();
    };
    if !solid(line) {
        return line.to_owned();
    }
    let rest = &line[at + 1..];
    let end = rest.find(']').map_or(rest.len(), |close| close + 1);
    format!("{}{}", &line[..at], &rest[end..])
}

/// Returns `line` without its brush, when every paint is solid or the brush only translates.
///
/// A brush that only translates is applied to the points a ramp is written with first.
fn without_brush(line: &str) -> String {
    let Some(at) = line.find(" brush=[") else {
        return line.to_owned();
    };
    let rest = &line[at + " brush=[".len()..];
    let Some(close) = rest.find(']') else {
        return line.to_owned();
    };
    let brush: Vec<f64> = rest[..close]
        .split(',')
        .filter_map(|number| number.trim().parse().ok())
        .collect();
    let without = format!("{}{}", &line[..at], &rest[close + 1..]);
    if solid(line) {
        return without;
    }
    let [1.0, 0.0, 0.0, 1.0, x, y] = brush[..] else {
        return line.to_owned();
    };
    ["from=(", "to=(", "at=("]
        .iter()
        .fold(without, |line, field| translated(&line, field, x, y))
}

/// Returns `line` with the point in `field` moved by `(x, y)`.
fn translated(line: &str, field: &str, x: f64, y: f64) -> String {
    let Some(at) = line.find(&format!(" {field}")) else {
        return line.to_owned();
    };
    let start = at + 1 + field.len();
    let Some(close) = line[start..].find(')') else {
        return line.to_owned();
    };
    let point: Vec<f64> = line[start..start + close]
        .split(',')
        .filter_map(|number| number.trim().parse().ok())
        .collect();
    let [px, py] = point[..] else {
        return line.to_owned();
    };
    format!(
        "{}{}, {}{}",
        &line[..start],
        number(px + x),
        number(py + y),
        &line[start + close..]
    )
}

/// Writes `value` to four decimals without trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}
