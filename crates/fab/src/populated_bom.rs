//! Generic populated component inventory paired with explicit calibrated placement.
use crate::{assembly::row_limited, assembly_templates::populated, snapshot::invalid};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io};
use viamind_doc::{ObjectId, Part};

pub(crate) fn validate(parts: &BTreeMap<ObjectId, Part>) -> io::Result<()> {
    let mut text = 0usize;
    let mut count = 0usize;
    for (id, part) in parts.iter().filter(|(_, part)| populated(part)) {
        if *id != part.id {
            return Err(invalid("populated BOM component map key differs from its stable ID"));
        }
        count += 1;
        if count > viamind_doc::assembly::MAX_SOURCING_COMPONENTS {
            return Err(invalid("populated BOM supports at most 100000 components"));
        }
        if let Some(metadata) = &part.assembly {
            metadata.validate_sourcing().map_err(invalid)?;
            for value in metadata.strings() {
                text = text.saturating_add(value.len());
                if text > viamind_doc::assembly::MAX_SOURCING_TEXT {
                    return Err(invalid("populated BOM sourcing exceeds 2 MiB aggregate UTF-8 text"));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn render(parts: &BTreeMap<ObjectId, Part>, limit: usize) -> io::Result<(String, usize, String)> {
    validate(parts)?;
    let limit = limit.min(32 * 1024 * 1024);
    let mut output = String::new();
    let mut digest = Sha256::new();
    digest.update(b"viamind-populated-components-v1\0");
    let mut count = 0usize;
    row_limited(
        &mut output,
        &["Designator", "Quantity", "Value", "Footprint", "Mounting", "Manufacturer", "MPN", "Supplier", "Supplier SKU", "Component ID"],
        limit,
    )?;
    for (id, part) in parts.iter().filter(|(_, part)| populated(part)) {
        let metadata = part.assembly.as_ref().ok_or_else(|| invalid("populated BOM requires explicit mounting and calibration"))?;
        let mounting = metadata.mounting.ok_or_else(|| invalid("populated BOM requires explicit mounting"))?;
        row_limited(
            &mut output,
            &[
                &part.designator,
                "1",
                &part.value,
                &part.footprint,
                mounting.name(),
                metadata.manufacturer.as_deref().unwrap_or(""),
                metadata.mpn.as_deref().unwrap_or(""),
                metadata.supplier.as_deref().unwrap_or(""),
                metadata.supplier_sku.as_deref().unwrap_or(""),
                &id.to_string(),
            ],
            limit,
        )?;
        digest.update(id.0.as_bytes());
        count += 1;
    }
    Ok((output, count, digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()))
}
