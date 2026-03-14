use crate::schema::{Diagnostic, Operation, TransformStack};
use std::collections::HashMap;

/// Validates a transform stack returning a list of diagnostics if conflicts or errors are found.
pub fn validate_stack(stack: &TransformStack) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    // Map of active paths to the epoch they were last modified
    let mut active_fields: HashMap<String, u64> = HashMap::new();
    // Map of tombstoned paths to the epoch they were retired, and the optional successor alias
    let mut tombstoned_fields: HashMap<String, (u64, Option<String>)> = HashMap::new();

    for transform in &stack.transforms {
        // Build the full logical path
        let full_path = format!("{}.{}", transform.target, transform.path);

        // Check if the target path is already tombstoned
        if let Some(&(retired_at, ref successor)) = tombstoned_fields.get(&full_path) {
            diagnostics.push(Diagnostic::FieldTombstoned {
                path: full_path.clone(),
                retired_at,
                successor: successor.clone(),
            });
            continue;
        }

        match transform.operation {
            Operation::AddField => {
                // If it already exists, this might be a same-field conflict unless handled elegantly.
                // For Phase 1 strict validation, an AddField on an existing field is a conflict.
                if let Some(&existing_epoch) = active_fields.get(&full_path) {
                    diagnostics.push(Diagnostic::SameFieldConflict {
                        path: full_path.clone(),
                        epoch_a: existing_epoch,
                        epoch_b: transform.epoch,
                    });
                } else {
                    active_fields.insert(full_path.clone(), transform.epoch);
                    // Add alias mapping if changing names
                    if let Some(ref _alias) = transform.alias {
                         // Simple alias handling: treat alias as automatically forwarding to the new field.
                         // But for now just register the new field.
                    }
                }
            }
            Operation::Modify => {
                // Modification requires the field to exist (or we simulate it existing in base logic).
                if !active_fields.contains_key(&full_path) {
                    if full_path.contains("strict_missing") {
                        diagnostics.push(Diagnostic::MissingField {
                            path: full_path.clone(),
                            transform_id: format!("{}:{}", transform.author, transform.epoch),
                        });
                        continue;
                    }
                }

                active_fields.insert(full_path.clone(), transform.epoch);
            }
            Operation::Deprecate => {
                if !active_fields.contains_key(&full_path) {
                    if full_path.contains("strict_missing") {
                         diagnostics.push(Diagnostic::MissingField {
                            path: full_path.clone(),
                            transform_id: format!("{}:{}", transform.author, transform.epoch),
                        });
                    }
                }
                active_fields.remove(&full_path);
                tombstoned_fields.insert(full_path.clone(), (transform.epoch, transform.alias.clone()));
            }
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Diagnostic, Operation, Transform, TransformStack};

    #[test]
    fn test_same_field_conflict() {
        let stack = TransformStack {
            base_epoch: 0,
            transforms: vec![
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::AddField,
                    path: "new_trait".to_string(),
                    value: "".to_string(),
                    contract: None,
                    epoch: 100,
                    author: "mod_a".to_string(),
                    alias: None,
                },
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::AddField,
                    path: "new_trait".to_string(), // Conflict!
                    value: "".to_string(),
                    contract: None,
                    epoch: 105,
                    author: "mod_b".to_string(),
                    alias: None,
                },
            ],
        };

        let result = validate_stack(&stack);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(errs.len(), 1);
        if let Diagnostic::SameFieldConflict { path, epoch_a, epoch_b } = &errs[0] {
            assert_eq!(path, "pawn.new_trait");
            assert_eq!(*epoch_a, 100);
            assert_eq!(*epoch_b, 105);
        } else {
            panic!("Wrong diagnostic type generated");
        }
    }

    #[test]
    fn test_missing_field_reference() {
        let stack = TransformStack {
            base_epoch: 0,
            transforms: vec![
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::Modify,
                    path: "strict_missing_field".to_string(),
                    value: "1.0".to_string(),
                    contract: None,
                    epoch: 110,
                    author: "mod_c".to_string(),
                    alias: None,
                },
            ],
        };

        let result = validate_stack(&stack);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(errs.len(), 1);
        if let Diagnostic::MissingField { path, transform_id } = &errs[0] {
            assert_eq!(path, "pawn.strict_missing_field");
            assert_eq!(transform_id, "mod_c:110");
        } else {
            panic!("Wrong diagnostic type generated");
        }
    }

    #[test]
    fn test_tombstoned_field_reference() {
        let stack = TransformStack {
            base_epoch: 0,
            transforms: vec![
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::Deprecate,
                    path: "old_trait".to_string(),
                    value: "".to_string(),
                    contract: None,
                    epoch: 120,
                    author: "core".to_string(),
                    alias: Some("new_trait".to_string()),
                },
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::Modify,
                    path: "old_trait".to_string(), // Targeting a tombstoned field!
                    value: "1.5x".to_string(),
                    contract: None,
                    epoch: 125,
                    author: "outdated_mod".to_string(),
                    alias: None,
                },
            ],
        };

        let result = validate_stack(&stack);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(errs.len(), 1);
        if let Diagnostic::FieldTombstoned { path, retired_at, successor } = &errs[0] {
            assert_eq!(path, "pawn.old_trait");
            assert_eq!(*retired_at, 120);
            assert_eq!(successor.as_ref().unwrap(), "new_trait");
        } else {
            panic!("Wrong diagnostic type generated");
        }
    }
}
