//! Shared document/session actions. UI entry points route changes through this catalog.
//!
//! The declaration generates both the dispatcher and its complete enumeration for the MCP
//! parity gate. View preferences (camera, layers, tool choice, test scene) are local UI state.

use serde_json::{Value, json};
use viamind_engine::{AddPart, Command, EngineError, EngineHandle, Origin, SetSelection, input_schema};

macro_rules! ui_actions {
    ($( $variant:ident => ($tool:expr, $schema:expr, $handler:ident) ),+ $(,)?) => {
        /// Every action the desktop can apply to the shared design or session.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum UiAction { $( $variant ),+ }

        impl UiAction {
            /// Generated from the dispatcher declaration, so new actions enter the parity gate.
            pub const ALL: &'static [Self] = &[$( Self::$variant ),+];

            pub fn tool(self) -> &'static str {
                match self { $( Self::$variant => $tool ),+ }
            }

            pub fn input_schema(self) -> Value {
                match self { $( Self::$variant => $schema ),+ }
            }

            /// Run as the human. Agents use the same engine, with their own origin and undo scope.
            pub fn run(self, engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
                match self { $( Self::$variant => $handler(engine, input) ),+ }
            }
        }
    };
}

fn no_args() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

ui_actions! {
    PlacePart => (AddPart::NAME, input_schema::<AddPart>(), place_part),
    Undo => ("undo", no_args(), undo),
    Redo => ("redo", no_args(), redo),
    SetSelection => ("set_selection", input_schema::<SetSelection>(), set_selection),
}

fn encode_error(error: serde_json::Error) -> EngineError {
    EngineError::InvalidInput { command: "ui_action".into(), message: error.to_string() }
}

fn place_part(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let result = engine.execute(AddPart::NAME, input, Origin::User)?;
    serde_json::to_value(result).map_err(encode_error)
}

fn undo(engine: &EngineHandle, _input: Value) -> Result<Value, EngineError> {
    engine.undo(Origin::User).map(|tx| json!({ "undone": tx }))
}

fn redo(engine: &EngineHandle, _input: Value) -> Result<Value, EngineError> {
    engine.redo(Origin::User).map(|tx| json!({ "redone": tx }))
}

fn set_selection(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input =
        serde_json::from_value(input).map_err(|error| EngineError::InvalidInput { command: "set_selection".into(), message: error.to_string() })?;
    let result = engine.set_selection(input, Origin::User)?;
    serde_json::to_value(result).map_err(encode_error)
}
