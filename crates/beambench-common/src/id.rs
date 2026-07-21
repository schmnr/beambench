use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use uuid::Uuid;

/// A typed UUID wrapper that prevents mixing IDs of different entity types.
/// `Id<Project>` and `Id<Layer>` are distinct types at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: Uuid,
    #[serde(skip)]
    _phantom: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new() -> Self {
        Self {
            value: Uuid::new_v4(),
            _phantom: PhantomData,
        }
    }

    pub fn from_uuid(value: Uuid) -> Self {
        Self {
            value,
            _phantom: PhantomData,
        }
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.value
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::fmt::Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Project;
    #[derive(Debug, PartialEq)]
    struct Layer;

    #[test]
    fn id_generates_unique_values() {
        let a = Id::<Project>::new();
        let b = Id::<Project>::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_roundtrips_through_json() {
        let id = Id::<Project>::new();
        let json = serde_json::to_string(&id).unwrap();
        let restored: Id<Project> = serde_json::from_str(&json).unwrap();
        assert_eq!(id, restored);
    }

    #[test]
    fn id_display_shows_uuid() {
        let id = Id::<Layer>::new();
        let display = format!("{}", id);
        assert_eq!(display, id.as_uuid().to_string());
    }
}
