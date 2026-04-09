use crate::audio::node::{Connection, NodeId, NodeInternalState, NodeType};

#[derive(Debug, Clone, PartialEq)]
pub enum UndoAction {
    AddedNode {
        node_id: NodeId,
        node_type: NodeType,
        position: (f32, f32),
    },
    RemovedNode {
        node_id: NodeId,
        node_type: NodeType,
        position: (f32, f32),
        enabled: bool,
        bypassed: bool,
        state: NodeInternalState,
        connections: Vec<Connection>,
    },
    Connected(Connection),
    Disconnected(Connection),
    MovedNode {
        node_id: NodeId,
        old_pos: (f32, f32),
        new_pos: (f32, f32),
    },
    ChangedState {
        node_id: NodeId,
        old_state: NodeInternalState,
        new_state: NodeInternalState,
    },
    ChangedBypass {
        node_id: NodeId,
        old_bypassed: bool,
        new_bypassed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct UndoStep {
    pub label: String,
    pub actions: Vec<UndoAction>,
    pub is_continuous: bool,
}

pub struct UndoManager {
    undo_stack: Vec<UndoStep>,
    redo_stack: Vec<UndoStep>,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, step: UndoStep) {
        if step.is_continuous {
            if let Some(top) = self.undo_stack.last_mut() {
                if top.is_continuous && top.actions.len() == step.actions.len() {
                    let same_nodes = top.actions.iter().zip(step.actions.iter()).all(|(a, b)| {
                        matches!(
                            (a, b),
                            (
                                UndoAction::ChangedState { node_id: id1, .. },
                                UndoAction::ChangedState { node_id: id2, .. }
                            ) if id1 == id2
                        )
                    });
                    if same_nodes {
                        for (existing, incoming) in top.actions.iter_mut().zip(step.actions.iter())
                        {
                            if let (
                                UndoAction::ChangedState { new_state, .. },
                                UndoAction::ChangedState {
                                    new_state: incoming_new_state,
                                    ..
                                },
                            ) = (existing, incoming)
                            {
                                *new_state = incoming_new_state.clone();
                            }
                        }
                        top.label = step.label;
                        self.redo_stack.clear();
                        return;
                    }
                }
            }
        }
        self.undo_stack.push(step);
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn pop_undo(&mut self) -> Option<UndoStep> {
        let step = self.undo_stack.pop()?;
        self.redo_stack.push(step.clone());
        Some(step)
    }

    pub fn pop_redo(&mut self) -> Option<UndoStep> {
        let step = self.redo_stack.pop()?;
        self.undo_stack.push(step.clone());
        Some(step)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::node::PortId;

    #[test]
    fn test_undo_manager_push_and_pop() {
        let mut mgr = UndoManager::new();
        assert!(!mgr.can_undo());
        assert!(!mgr.can_redo());

        mgr.push(UndoStep {
            label: "test".into(),
            actions: vec![UndoAction::Connected(Connection {
                source_node: NodeId(1),
                source_port: PortId(0),
                target_node: NodeId(2),
                target_port: PortId(0),
            })],
            is_continuous: false,
        });
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());

        let step = mgr.pop_undo().unwrap();
        assert_eq!(step.label, "test");
        assert!(!mgr.can_undo());
        assert!(mgr.can_redo());

        let step = mgr.pop_redo().unwrap();
        assert_eq!(step.label, "test");
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());
    }

    #[test]
    fn test_undo_clears_redo_on_push() {
        let mut mgr = UndoManager::new();
        mgr.push(UndoStep {
            label: "a".into(),
            actions: vec![],
            is_continuous: false,
        });
        mgr.push(UndoStep {
            label: "b".into(),
            actions: vec![],
            is_continuous: false,
        });
        let _ = mgr.pop_undo();
        assert!(mgr.can_redo());

        mgr.push(UndoStep {
            label: "c".into(),
            actions: vec![],
            is_continuous: false,
        });
        assert!(!mgr.can_redo());
    }

    #[test]
    fn test_continuous_coalescing() {
        let mut mgr = UndoManager::new();

        mgr.push(UndoStep {
            label: "drag1".into(),
            actions: vec![UndoAction::ChangedState {
                node_id: NodeId(1),
                old_state: NodeInternalState::Gain { value: 0.5 },
                new_state: NodeInternalState::Gain { value: 0.6 },
            }],
            is_continuous: true,
        });
        assert!(mgr.can_undo());

        mgr.push(UndoStep {
            label: "drag2".into(),
            actions: vec![UndoAction::ChangedState {
                node_id: NodeId(1),
                old_state: NodeInternalState::Gain { value: 0.6 },
                new_state: NodeInternalState::Gain { value: 0.7 },
            }],
            is_continuous: true,
        });

        assert!(mgr.can_undo());
        let step = mgr.pop_undo().unwrap();
        assert_eq!(step.label, "drag2");
        assert_eq!(
            step.actions[0],
            UndoAction::ChangedState {
                node_id: NodeId(1),
                old_state: NodeInternalState::Gain { value: 0.5 },
                new_state: NodeInternalState::Gain { value: 0.7 },
            }
        );

        let redo_step = mgr.pop_redo().unwrap();
        assert_eq!(redo_step.label, "drag2");
        assert_eq!(
            redo_step.actions[0],
            UndoAction::ChangedState {
                node_id: NodeId(1),
                old_state: NodeInternalState::Gain { value: 0.5 },
                new_state: NodeInternalState::Gain { value: 0.7 },
            }
        );
    }

    #[test]
    fn test_continuous_no_coalesce_different_node() {
        let mut mgr = UndoManager::new();

        mgr.push(UndoStep {
            label: "a".into(),
            actions: vec![UndoAction::ChangedState {
                node_id: NodeId(1),
                old_state: NodeInternalState::Gain { value: 0.5 },
                new_state: NodeInternalState::Gain { value: 0.6 },
            }],
            is_continuous: true,
        });

        mgr.push(UndoStep {
            label: "b".into(),
            actions: vec![UndoAction::ChangedState {
                node_id: NodeId(2),
                old_state: NodeInternalState::Gain { value: 0.5 },
                new_state: NodeInternalState::Gain { value: 0.6 },
            }],
            is_continuous: true,
        });

        let step = mgr.pop_undo().unwrap();
        assert_eq!(step.label, "b");
        let step = mgr.pop_undo().unwrap();
        assert_eq!(step.label, "a");
    }

    #[test]
    fn test_clear() {
        let mut mgr = UndoManager::new();
        mgr.push(UndoStep {
            label: "a".into(),
            actions: vec![],
            is_continuous: false,
        });
        mgr.clear();
        assert!(!mgr.can_undo());
        assert!(!mgr.can_redo());
    }
}
