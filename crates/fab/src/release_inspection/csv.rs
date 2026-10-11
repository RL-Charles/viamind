//! Original bounded CSV reader for the exact exported calibrated/populated dialect.
use super::{MatchedAssembly, invalid};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
use viamind_doc::ObjectId;

const PLACEMENT: [&str; 9] =
    ["Designator", "Component ID", "Value", "Footprint", "Mounting", "X (mm, global)", "Y (mm, up)", "Rotation (deg CCW, top view)", "Side"];
const BOM: [&str; 10] =
    ["Designator", "Quantity", "Value", "Footprint", "Mounting", "Manufacturer", "MPN", "Supplier", "Supplier SKU", "Component ID"];
#[derive(Debug, PartialEq, Eq)]
struct Component {
    reference: String,
    value: String,
    footprint: String,
    mounting: String,
}

/// Fixed scale, checked arithmetic only. No exponent, whitespace or nonfinite syntax.
fn decimal(text: &str, scale: u32, min: i64, max: i64) -> io::Result<i64> {
    let (negative, text) = text.strip_prefix('-').map_or((false, text), |s| (true, s));
    let mut parts = text.split('.');
    let whole = parts.next().unwrap_or("");
    let fractional = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fractional.len() > scale as usize
        || !fractional.bytes().all(|b| b.is_ascii_digit())
        || text.ends_with('.')
    {
        return Err(invalid("invalid fixed decimal CSV coordinate/rotation"));
    }
    let power = 10_i64.pow(scale);
    let value = whole
        .parse::<i64>()
        .ok()
        .and_then(|v| v.checked_mul(power))
        .and_then(|v| {
            fractional
                .parse::<i64>()
                .or_else(|e| if fractional.is_empty() { Ok(0) } else { Err(e) })
                .ok()
                .and_then(|f| f.checked_mul(10_i64.pow(scale - fractional.len() as u32)))
                .and_then(|f| v.checked_add(f))
        })
        .and_then(|v| if negative { v.checked_neg() } else { Some(v) })
        .filter(|v| (min..=max).contains(v))
        .ok_or_else(|| invalid("CSV coordinate/rotation outside checked integer bounds"))?;
    Ok(value)
}

/// Process one row at a time. Field growth and number of rows/columns are charged first.
fn rows(bytes: &[u8], headers: &[&str], mut consume: impl FnMut(Vec<String>) -> io::Result<()>) -> io::Result<()> {
    std::str::from_utf8(bytes).map_err(|_| invalid("assembly CSV must be UTF-8"))?;
    let mut index = 0;
    let mut count = 0;
    while index < bytes.len() {
        let mut fields = Vec::with_capacity(headers.len());
        loop {
            if fields.len() >= headers.len() {
                return Err(invalid("CSV has excess columns"));
            }
            let mut field = Vec::new();
            let quoted = bytes.get(index) == Some(&b'"');
            if quoted {
                index += 1;
            }
            let mut closed = !quoted;
            while index < bytes.len() {
                let b = bytes[index];
                if quoted && b == b'"' {
                    index += 1;
                    if bytes.get(index) == Some(&b'"') {
                        if field.len() >= 4096 {
                            return Err(invalid("CSV field exceeds 4096 bytes"));
                        }
                        field.push(b'"');
                        index += 1;
                        continue;
                    }
                    closed = true;
                    break;
                }
                if !quoted && matches!(b, b',' | b'\r' | b'\n') {
                    break;
                }
                if !quoted && b == b'"' {
                    return Err(invalid("CSV quote in unquoted field"));
                }
                if field.len() >= 4096 {
                    return Err(invalid("CSV field exceeds 4096 bytes"));
                }
                field.push(b);
                index += 1;
            }
            if !closed {
                return Err(invalid("unterminated quoted CSV field"));
            }
            fields.push(String::from_utf8(field).map_err(|_| invalid("CSV field boundary splits UTF-8"))?);
            match bytes.get(index) {
                Some(b',') => {
                    index += 1;
                }
                Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => {
                    index += 2;
                    break;
                }
                None => break,
                _ => return Err(invalid("CSV records require CRLF and no trailing quoted junk")),
            }
        }
        if fields.len() != headers.len() {
            return Err(invalid("CSV has wrong column count"));
        }
        if count == 0 {
            if !fields.iter().zip(headers).all(|(a, b)| a == b) {
                return Err(invalid("CSV header differs from supported export"));
            }
        } else {
            if count > 100_000 {
                return Err(invalid("assembly CSV exceeds 100000 component rows"));
            }
            consume(fields)?;
        }
        count += 1;
    }
    if count == 0 {
        return Err(invalid("assembly CSV missing header"));
    }
    Ok(())
}
fn component(fields: &[String]) -> io::Result<Component> {
    let reference = &fields[0];
    if reference.is_empty()
        || reference.len() > 64
        || !reference.as_bytes()[0].is_ascii_alphabetic()
        || !reference.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(invalid("calibrated CSV reference is not a supported explicit designator"));
    }
    if !matches!(fields[4].as_str(), "surface_mount" | "through_hole" | "hybrid") {
        return Err(invalid("assembly CSV requires explicit supported mounting"));
    }
    Ok(Component { reference: reference.clone(), value: fields[2].clone(), footprint: fields[3].clone(), mounting: fields[4].clone() })
}
fn identifier(text: &str) -> io::Result<()> {
    if text.len() > 512 || text.trim() != text || text.chars().any(char::is_control) {
        return Err(invalid("populated BOM sourcing identifier outside explicit-field bounds"));
    }
    Ok(())
}
pub(super) fn inspect(placement: &[u8], bom: Option<&[u8]>, count: usize, digest: Option<&str>) -> io::Result<Option<MatchedAssembly>> {
    let mut parts = BTreeMap::new();
    let mut refs = BTreeSet::new();
    rows(placement, &PLACEMENT, |fields| {
        let part = component(&fields)?;
        if !refs.insert(part.reference.to_ascii_uppercase()) {
            return Err(invalid("duplicate calibrated CSV reference"));
        }
        let id = fields[1].parse::<ObjectId>().map_err(|_| invalid("invalid calibrated component UUID"))?;
        decimal(&fields[5], 6, -99_999_999_999, 99_999_999_999)?;
        decimal(&fields[6], 6, -99_999_999_999, 99_999_999_999)?;
        decimal(&fields[7], 3, 0, 359_999)?;
        if !matches!(fields[8].as_str(), "top" | "bottom") {
            return Err(invalid("invalid calibrated CSV side"));
        }
        if parts.insert(id, part).is_some() {
            return Err(invalid("duplicate calibrated component UUID"));
        }
        Ok(())
    })?;
    if parts.len() != count {
        return Err(invalid("calibrated component count differs from manifest"));
    }
    let Some(bom) = bom else { return Ok(None) };
    let mut hash = Sha256::new();
    hash.update(b"viamind-populated-components-v1\0");
    let mut previous = None;
    let mut bom_count = 0;
    rows(bom, &BOM, |fields| {
        let id = fields[9].parse::<ObjectId>().map_err(|_| invalid("invalid populated component UUID"))?;
        if previous.is_some_and(|last| last >= id) {
            return Err(invalid("populated BOM UUIDs must be unique and ascending"));
        }
        previous = Some(id);
        if fields[1] != "1" {
            return Err(invalid("populated BOM requires quantity 1 per component"));
        }
        for field in &fields[5..9] {
            identifier(field)?;
        }
        let part = component(&fields)?;
        if parts.get(&id) != Some(&part) {
            return Err(invalid("populated BOM fields/identity differ from calibrated placement"));
        }
        hash.update(id.as_bytes());
        bom_count += 1;
        Ok(())
    })?;
    let observed = hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    if bom_count != count || digest != Some(observed.as_str()) {
        return Err(invalid("matched populated count/component identity digest differs from manifest"));
    }
    Ok(Some(MatchedAssembly { component_count: count, component_ids_sha256: observed }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_decimals_reject_exponents_nonfinite_and_overflow() {
        for s in ["1e2", "NaN", "inf", "+1", "1.", "1.0000001", "9223372036854775807", "100000.000000", " 1"] {
            assert!(decimal(s, 6, -99_999_999_999, 99_999_999_999).is_err(), "{s}");
        }
        assert_eq!(decimal("-99999.999999", 6, -99_999_999_999, 99_999_999_999).unwrap(), -99_999_999_999);
        assert!(decimal("360", 3, 0, 359_999).is_err());
    }
    #[test]
    fn quoted_unicode_crlf_and_multiline_fields_are_preserved() {
        let mut observed = Vec::new();
        rows("A,B\r\n\"µ, \"\"q\"\"\r\nline\",x\r\n".as_bytes(), &["A", "B"], |row| {
            observed.push(row);
            Ok(())
        })
        .unwrap();
        assert_eq!(observed[0][0], "µ, \"q\"\r\nline");
        for input in ["A,B\r\n\"bad\"x,z\r\n", "A,B\r\n\"bad,z", "A,B\n", "A,B\r\nx,\"y\"\r\nextra", "A,B\r\nx,y,z\r\n"] {
            assert!(rows(input.as_bytes(), &["A", "B"], |_| Ok(())).is_err(), "{input}");
        }
    }
}
