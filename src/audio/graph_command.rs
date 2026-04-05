use crate::audio::node::{Connection, NodeId, NodeInternalState, NodeType, PortId};

#[derive(Debug)]
pub enum GraphCommand {
    AddNode(NodeType),
    RemoveNode(NodeId),
    SetNodeEnabled(NodeId, bool),
    SetNodeBypassed(NodeId, bool),
    SetNodeState(NodeId, NodeInternalState),
    SetNodePosition(NodeId, f32, f32),
    Connect(Connection),
    Disconnect {
        source: (NodeId, PortId),
        target: (NodeId, PortId),
    },
    CommitTopology,
}
