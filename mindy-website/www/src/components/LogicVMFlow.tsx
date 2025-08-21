import {
    addEdge,
    useNodesState,
    useEdgesState,
    ReactFlow,
    type Connection,
    type Edge,
    Background,
    BackgroundVariant,
    Controls,
    MarkerType,
    useReactFlow,
} from "@xyflow/react";
import { useCallback, useEffect } from "react";

import { DisplayKind, ProcessorKind } from "mindy-website";

import { useLogicVM } from "../hooks";
import { createNode } from "../utils";
import AddBuildingMenu from "./AddBuildingMenu";
import BuildingLinkConnectionLine from "./edges/BuildingLinkConnectionLine";
import BuildingLinkEdge from "./edges/BuildingLinkEdge";
import DisplayNode, { type DisplayNodeType } from "./nodes/DisplayNode";
import MemoryNode, { type MemoryNodeType } from "./nodes/MemoryNode";
import MessageNode, { type MessageNodeType } from "./nodes/MessageNode";
import ProcessorNode, { type ProcessorNodeType } from "./nodes/ProcessorNode";
import SwitchNode, { type SwitchNodeType } from "./nodes/SwitchNode";

export type LogicVMNode =
    | DisplayNodeType
    | MemoryNodeType
    | MessageNodeType
    | ProcessorNodeType
    | SwitchNodeType;

const nodeTypes = {
    display: DisplayNode,
    memory: MemoryNode,
    message: MessageNode,
    processor: ProcessorNode,
    switch: SwitchNode,
};

const edgeTypes = {
    buildinglink: BuildingLinkEdge,
};

const defaultEdgeOptions: Partial<Edge> = {
    type: "buildinglink",
    markerEnd: { type: MarkerType.Arrow },
};

const defaultNodes: LogicVMNode[] = [
    createNode({
        type: "display",
        position: { x: 400, y: 0 },
        data: {
            position: { x: 0, y: 0 },
            kind: DisplayKind.Tiled,
            displayWidth: 256,
            displayHeight: 256,
        },
    }),
    // put the processor last to ensure the linked buildings exist
    createNode({
        type: "processor",
        position: { x: 0, y: 0 },
        data: {
            position: { x: 1, y: 0 },
            kind: ProcessorKind.World,
            defaultCode: `
sensor x display1 @displayWidth
op div x x 2
sensor y display1 @displayHeight
op div y y 2

print "Hello, world!"
draw print x y @center
drawflush display1

stop
            `.trim(),
        },
    }),
];

const defaultEdges: Edge[] = [
    {
        id: "processor-display",
        source: defaultNodes[1].id,
        target: defaultNodes[0].id,
    },
];

export default function LogicVMFlow() {
    const vm = useLogicVM();

    const reactFlow = useReactFlow<LogicVMNode>();

    const [nodes, _setNodes, onNodesChange] =
        useNodesState<LogicVMNode>(defaultNodes);

    const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(defaultEdges);

    const onConnect = useCallback(
        (params: Edge | Connection) => {
            // https://github.com/xyflow/xyflow/blob/a75087e8d3a6ea0731f5bd2331027dc89edce85c/packages/system/src/utils/edges/general.ts#L91
            const existingEdge = reactFlow
                .getNodeConnections({
                    type: "source",
                    nodeId: params.source,
                    handleId: params.sourceHandle,
                })
                .find(
                    (edge) =>
                        edge.target === params.target
                        && (edge.targetHandle === params.targetHandle
                            || (!edge.targetHandle && !params.targetHandle)),
                );
            if (existingEdge != null) {
                // toggle on double connect, for better mobile support
                setEdges((edgesSnapshot) =>
                    edgesSnapshot.filter(
                        (edge) => edge.id !== existingEdge.edgeId,
                    ),
                );
            } else {
                setEdges((edgesSnapshot) => addEdge(params, edgesSnapshot));
            }
        },
        [setEdges, reactFlow],
    );

    useEffect(() => {
        vm.onmessage = ({ data }) => {
            console.debug("Got response:", data);
        };
        vm.onmessageerror = (event) => {
            console.error("Failed to deserialize message from VM:", event);
        };
        vm.onerror = (event) => {
            console.error("Got error from VM:", event);
        };

        return () => {
            vm.onmessage = null;
            vm.onmessageerror = null;
            vm.onerror = null;
        };
    }, [vm, reactFlow]);

    return (
        <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            defaultEdgeOptions={defaultEdgeOptions}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            connectionLineComponent={BuildingLinkConnectionLine}
            proOptions={{ hideAttribution: true }}
            nodeOrigin={[0.5, 0.5]}
            fitView
        >
            <Background variant={BackgroundVariant.Dots} />
            <Controls />
            <AddBuildingMenu />
        </ReactFlow>
    );
}
