# Pin and pad cross-probing

Select one PCB component or schematic symbol. In **Linked component**, choose
**Inspect pins and pads**, then choose a terminal row. Cyan rings mark the paired
schematic endpoint and explicit PCB pad center in their respective editors.
**Show schematic pin** and **Show PCB pad** center the view explicitly. Terminal
choices and rings are local view state; shared PCB component selection stays as it
was. Hidden destination copper rejects navigation and remains hidden. Finish or
cancel active edits before using these navigation buttons.

Mappings use only the explicit stable symbol link and exact unique terminal
numbers. References and net names do not establish mappings. Every pin number
must have exactly one pad number and vice versa before navigation is available.
Unlinked, missing-symbol, legacy preview-pad, mismatched and empty components have
explicit coverage. PCB center navigation currently supports quarter-turn rotations
and bounded integer coordinates; other angles remain visible design geometry but
are reported as unsupported for exact center navigation.

A peer edit, new source selection or document replacement invalidates the result.
Reinspect after changes, including reopening a file with the same project UUID.
The panel retains at most one outstanding worker and displays at most 100 rows
per page. Use the terminal page controls for larger supported components.

## Agent query

```json
{
  "name": "query_linked_pin_pads",
  "arguments": {"component_id": "<part-or-symbol-uuid>", "offset": 0, "limit": 100}
}
```

The response includes `project_id`, `source_cursor`, the stable `component`,
`coverage`, ordered `terminals`, `total` and `next_offset`. Rows supply exact integer
Nm positions, pin and pad UUIDs, side, `navigable`, and `unsupported_reasons`.
Restart pagination after the full event cursor changes. A read never changes the
project, event history, dirty state or selection.

The inspected structure is limited to 10000 combined sheets, symbols, parts,
pins and explicit pads, 4096 bytes per inspected sheet name, reference, pin name or terminal string and 2 MiB total inspected
text. Limit errors reject before allocation of identity maps or string copies.
Unrelated wires, labels, copper, values and footprint/net strings are not validated
or copied by this query. Physical mapping does not establish electrical continuity,
net highlighting, ERC or manufacturing suitability.
