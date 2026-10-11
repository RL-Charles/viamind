# Create a reusable native symbol

In **Schematic**, open **Native symbol library…** and select **New detached definition**.
To copy an existing native definition, load it first and select **Edit loaded
definition**.

Edit the name, default value, optional footprint, rectangular body dimensions and
ordered pins. Each pin has a number, name, local coordinates and electrical type.
**Add definition pin** chooses an unused number; **Remove pin** removes that local
row. Dimensions and coordinates use decimal millimetres with up to six fractional
digits, so one nanometre is preserved exactly.

The form is local. Typing and validation preserve the project, history and placed
instances. **Use edited definition** selects the validated definition for fresh
placement. Refresh the target sheet after project edits, enter a unique reference
and explicit placement/rotation/mirror, then select **Place new library symbol**.
That placement is one undoable transaction with fresh symbol and pin identities.

**Save NEW definition file** writes the validated local definition atomically to
a new `.viamind-symbol.json` file. Existing files and the native project are
protected. Saving does not select the edited definition for placement. Errors
preserve the form so you can correct a field or choose another path. The library
and pin list scroll in compact windows; pending file work disables duplicate
actions and field edits.

The shared MCP action is `save_symbol_template` with `{template, path}`. Its
`template` uses the same detached fields as `load_symbol_template`: name, value,
optional footprint, width/height in integer Nm, and ordered pins with number,
name, local integer Nm position and electrical type. Save returns canonical
path, name and pin count. Use `snapshot_symbol_sheet` and
`place_symbol_template` for guarded placement.

This format supports one rectangular single-unit symbol, at most 1024 pins,
4096 bytes per text field, 256 KiB aggregate text and 1 MiB serialized output.
The desktop also applies the native name/value character limits. Multi-unit
packages, arbitrary vector graphics and external symbol formats remain open.
