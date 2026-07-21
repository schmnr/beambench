//! Macro definitions for G-code automation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User-defined macro for automating G-code sequences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDefinition {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub show_in_toolbar: bool,
}

impl Default for MacroDefinition {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            description: String::new(),
            commands: Vec::new(),
            hotkey: None,
            show_in_toolbar: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_definition_default_values() {
        let macro_def = MacroDefinition::default();
        assert_eq!(macro_def.name, "");
        assert_eq!(macro_def.description, "");
        assert!(macro_def.commands.is_empty());
    }

    #[test]
    fn macro_definition_roundtrips() {
        let macro_def = MacroDefinition {
            id: Uuid::new_v4(),
            name: "Home and Frame".to_string(),
            description: "Home the machine and frame the workpiece".to_string(),
            commands: vec!["$H".to_string(), "$J=G91 X100 F1000".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };
        let json = serde_json::to_string(&macro_def).unwrap();
        let restored: MacroDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(macro_def, restored);
    }

    #[test]
    fn macro_definition_ids_are_unique() {
        let m1 = MacroDefinition::default();
        let m2 = MacroDefinition::default();
        assert_ne!(m1.id, m2.id);
    }

    #[test]
    fn macro_definition_multiple_commands() {
        let macro_def = MacroDefinition {
            id: Uuid::new_v4(),
            name: "Air Assist On".to_string(),
            description: "Enable air assist".to_string(),
            commands: vec!["M106 S255".to_string(), "G4 P0.5".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };
        assert_eq!(macro_def.commands.len(), 2);
        assert_eq!(macro_def.commands[0], "M106 S255");
        assert_eq!(macro_def.commands[1], "G4 P0.5");
    }

    #[test]
    fn macro_definition_empty_commands_roundtrips() {
        let macro_def = MacroDefinition {
            id: Uuid::new_v4(),
            name: "Empty".to_string(),
            description: "No commands".to_string(),
            commands: Vec::new(),
            hotkey: None,
            show_in_toolbar: false,
        };
        let json = serde_json::to_string(&macro_def).unwrap();
        let restored: MacroDefinition = serde_json::from_str(&json).unwrap();
        assert!(restored.commands.is_empty());
    }

    #[test]
    fn old_macro_without_hotkey_fields_deserializes() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Test",
            "description": "desc",
            "commands": ["G0 X0 Y0"]
        }"#;
        let restored: MacroDefinition = serde_json::from_str(json).unwrap();
        assert!(restored.hotkey.is_none());
        assert!(!restored.show_in_toolbar);
    }

    #[test]
    fn macro_with_hotkey_and_toolbar_roundtrips() {
        let macro_def = MacroDefinition {
            id: Uuid::new_v4(),
            name: "Quick Home".to_string(),
            description: "Home the machine".to_string(),
            commands: vec!["$H".to_string()],
            hotkey: Some("Ctrl+1".to_string()),
            show_in_toolbar: true,
        };
        let json = serde_json::to_string(&macro_def).unwrap();
        let restored: MacroDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(macro_def, restored);
        assert_eq!(restored.hotkey, Some("Ctrl+1".to_string()));
        assert!(restored.show_in_toolbar);
    }
}
