use bevy::prelude::*;
use simrard_lib_pawn::NeuralNetworkComponent;
use std::fs;
use std::time::SystemTime;

use crate::schema::{Operation, TransformStack};
use crate::validation::validate_stack;

#[derive(Resource)]
pub struct TransformWatcher {
    last_modified: SystemTime,
}

impl Default for TransformWatcher {
    fn default() -> Self {
        Self {
            last_modified: SystemTime::UNIX_EPOCH,
        }
    }
}

pub struct TransformsPlugin;

impl Plugin for TransformsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TransformWatcher>()
           .add_systems(Update, watch_and_solidify);
    }
}

fn watch_and_solidify(
    mut watcher: ResMut<TransformWatcher>,
    mut query: Query<&mut NeuralNetworkComponent>,
) {
    let dir = "transforms";
    if let Ok(metadata) = fs::metadata(dir) {
        if let Ok(modified) = metadata.modified() {
            if modified > watcher.last_modified {
                watcher.last_modified = modified;
                println!("Transforms directory changed. Resolifying...");

                // Read transforms/schema.json for phase 0 testing
                let file_path = format!("{}/test.json", dir);
                if let Ok(contents) = fs::read_to_string(&file_path) {
                    if let Ok(stack) = serde_json::from_str::<TransformStack>(&contents) {

                        // Phase 1: Transactional Contract Validation
                        if let Err(diagnostics) = validate_stack(&stack) {
                            eprintln!("Transform validation failed. Aborting hot-reload. Diagnostics: {:#?}", diagnostics);
                            return;
                        }

                        println!("Transform stack validated. Solidifying...");
                        let mut count = 0;
                        for mut nn in query.iter_mut() {
                            solidify(&stack, &mut nn);
                            count += 1;
                        }
                        if count > 0 {
                            println!("Solidified {} pawn(s).", count);
                        }
                    } else {
                        eprintln!("Failed to parse JSON transform stack");
                    }
                }
            }
        }
    }
}

fn solidify(stack: &TransformStack, nn: &mut NeuralNetworkComponent) {
    for transform in &stack.transforms {
        if transform.target == "pawn.needs" {
            // Very naive phase 0 dot-path parser
            if transform.path == "hunger.depletion_rate" {
                if let Operation::Modify = transform.operation {
                    if let Ok(val) = transform.value.replace("x", "").parse::<f32>() {
                        nn.hunger *= val;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Transform;
    use crate::validation::validate_stack;

    #[test]
    fn test_transactional_hot_reload_rollback() {
        // We simulate what the watcher does inside a test environment.
        let mut nn = NeuralNetworkComponent {
            hunger: 100.0,
            thirst: 100.0,
            fatigue: 100.0,
            curiosity: 0.0,
            social: 0.0,
            fear: 0.0,
            industriousness: 0.0,
            comfort: 100.0,
        };

        // Stack with a known missing field conflict
        let conflict_stack = TransformStack {
            base_epoch: 0,
            transforms: vec![
                Transform {
                    target: "pawn".to_string(),
                    operation: Operation::Modify,
                    path: "strict_missing_field".to_string(),
                    value: "2.0x".to_string(),
                    contract: None,
                    epoch: 150,
                    author: "bad_mod".to_string(),
                    alias: None,
                },
            ],
        };

        let result = validate_stack(&conflict_stack);
        assert!(result.is_err(), "Stack should fail validation");

        if result.is_ok() {
            solidify(&conflict_stack, &mut nn);
        }

        // The neural network state must be completely untouched (transactional rollback preserved prior state)
        assert_eq!(nn.hunger, 100.0);
    }
}
