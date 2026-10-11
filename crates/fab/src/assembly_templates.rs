//! Restricted, explicitly sourced BOM layouts; no placement or catalog lookup.
use crate::{AssemblyCsv, assembly::row_limited, snapshot::invalid};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
use viamind_doc::Part;

pub(crate) fn populated(part: &Part) -> bool {
    part.assembly.as_ref().is_none_or(|metadata| metadata.populate)
}
pub(crate) fn validate<'a>(parts: impl IntoIterator<Item = &'a Part>, kind: AssemblyCsv) -> io::Result<()> {
    let mut references = BTreeSet::new();
    for part in parts.into_iter().filter(|part| populated(part)) {
        let fail = |field, reason| invalid(format!("{}: component {:?} {field} {reason}", kind.name(), part.designator));
        let reference = part.designator.as_bytes();
        if reference.is_empty()
            || reference.len() > 64
            || !reference[0].is_ascii_alphabetic()
            || !reference.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return Err(fail("designator", "needs 1–64 ASCII letters/digits/underscores, starting with a letter"));
        }
        if !references.insert(part.designator.to_ascii_uppercase()) {
            return Err(fail("designator", "duplicates another included reference after ASCII case folding"));
        }
        let clean = |value: &str| value.trim() == value && !value.chars().any(char::is_control);
        if part.footprint.is_empty() || !clean(&part.footprint) {
            return Err(fail("footprint", "must be nonempty without control or edge whitespace"));
        }
        let metadata = part.assembly.as_ref();
        if kind == AssemblyCsv::CatalogBom {
            if part.value.is_empty() || !clean(&part.value) {
                return Err(fail("value", "must be nonempty without control or edge whitespace"));
            }
            let supplier = metadata.and_then(|metadata| metadata.supplier.as_deref()).unwrap_or("");
            if !supplier.eq_ignore_ascii_case("LCSC") && !supplier.eq_ignore_ascii_case("JLCPCB") {
                return Err(fail("supplier", "must explicitly name LCSC or JLCPCB for a catalog BOM"));
            }
            let sku = metadata.and_then(|metadata| metadata.supplier_sku.as_deref()).unwrap_or("");
            if !sku.starts_with('C') || sku.len() < 2 || !sku.as_bytes()[1..].iter().all(u8::is_ascii_digit) {
                return Err(fail("supplier SKU", "must be an explicit capital C followed by ASCII digits; no lookup is performed"));
            }
        } else {
            if metadata.and_then(|metadata| metadata.mpn.as_deref()).is_none() {
                return Err(fail("manufacturer part number", "is required and must be entered explicitly"));
            }
            if !clean(&part.value) {
                return Err(fail("value", "must have no control or edge whitespace"));
            }
        }
    }
    Ok(())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key<'a> {
    value: &'a str,
    footprint: &'a str,
    manufacturer: &'a str,
    mpn: &'a str,
    supplier: &'a str,
    supplier_sku: &'a str,
}
pub(crate) fn render(parts: &[&Part], kind: AssemblyCsv, limit: usize) -> io::Result<String> {
    let mut output = String::new();
    let row = |output: &mut String, fields: &[&str]| row_limited(output, fields, limit);
    if kind == AssemblyCsv::CatalogBom {
        row(&mut output, &["Comment", "Designator", "Footprint", "JLCPCB Part #（optional）"])?;
    } else {
        row(
            &mut output,
            &[
                "Item #",
                "*Designator",
                "*Qty",
                "Manufacturer",
                "*Mfg Part #",
                "Description / Value",
                "*Package/Footprint ",
                "Type",
                "Your Instructions / Notes",
            ],
        )?;
    }
    let mut groups: BTreeMap<Key<'_>, Vec<&Part>> = BTreeMap::new();
    for part in parts.iter().copied().filter(|part| populated(part)) {
        let metadata = part.assembly.as_ref();
        groups
            .entry(Key {
                value: &part.value,
                footprint: &part.footprint,
                manufacturer: metadata.and_then(|m| m.manufacturer.as_deref()).unwrap_or(""),
                mpn: metadata.and_then(|m| m.mpn.as_deref()).unwrap_or(""),
                supplier: metadata.and_then(|m| m.supplier.as_deref()).unwrap_or(""),
                supplier_sku: metadata.and_then(|m| m.supplier_sku.as_deref()).unwrap_or(""),
            })
            .or_default()
            .push(part);
    }
    let mut item = 0;
    for (key, parts) in groups {
        for chunk in parts.chunks(200) {
            item += 1;
            let references = chunk.iter().map(|part| part.designator.as_str()).collect::<Vec<_>>().join(",");
            if kind == AssemblyCsv::CatalogBom {
                row(&mut output, &[key.value, &references, key.footprint, key.supplier_sku])?;
            } else {
                row(
                    &mut output,
                    &[&item.to_string(), &references, &chunk.len().to_string(), key.manufacturer, key.mpn, key.value, key.footprint, "", ""],
                )?;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_bytes_are_charged_before_row_append() {
        let mut output = "prefix".to_owned();
        let expected = "prefix\"a,\"\"b\"\r\n";
        assert!(row_limited(&mut output, &["a,\"b"], expected.len() - 1).is_err());
        assert_eq!(output, "prefix");
        row_limited(&mut output, &["a,\"b"], expected.len()).unwrap();
        assert_eq!(output, expected);
        for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
            assert!(render(&[], kind, 1).is_err());
        }
    }
}
