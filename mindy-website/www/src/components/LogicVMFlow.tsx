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
} from "@xyflow/react";
import { useCallback } from "react";

import { pack_point, ProcessorKind, type WebLogicVM } from "mindy-website";

import BuildingLinkEdge from "./BuildingLinkEdge";
import DisplayNode, { type DisplayNodeType } from "./nodes/DisplayNode";
import ProcessorNode, { type ProcessorNodeType } from "./nodes/ProcessorNode";

export type CustomNodeType = ProcessorNodeType | DisplayNodeType;

const nodeTypes = {
    processor: ProcessorNode,
    display: DisplayNode,
};

const edgeTypes = {
    buildinglink: BuildingLinkEdge,
};

const defaultEdgeOptions: Partial<Edge> = {
    type: "buildinglink",
    markerEnd: { type: MarkerType.Arrow },
    // TODO: figure out how to calculate edge labels
    selectable: false,
};

interface VMFlowProps {
    vm: WebLogicVM;
}

export default function LogicVMFlow({ vm }: VMFlowProps) {
    const [nodes, _setNodes, onNodesChange] = useNodesState<CustomNodeType>([
        {
            id: "processor",
            type: "processor",
            position: { x: 0, y: 0 },
            data: {
                vm,
                position: pack_point(0, 0),
                kind: ProcessorKind.World,
            },
        },
        {
            id: "display",
            type: "display",
            position: { x: 400, y: 0 },
            data: {
                vm,
                position: pack_point(1, 0),
                displayWidth: 256,
                displayHeight: 256,
            },
        },
    ]);

    const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([
        {
            id: "processor-display",
            source: "processor",
            target: "display",
            label: "display1",
        },
    ]);

    // abort deletions that would remove nodes
    const onBeforeDelete = useCallback(
        // eslint-disable-next-line @typescript-eslint/require-await
        async ({ nodes }: { nodes: CustomNodeType[] }) => nodes.length === 0,
        [],
    );

    const onConnect = useCallback(
        (params: Edge | Connection) =>
            setEdges((edgesSnapshot) => addEdge(params, edgesSnapshot)),
        [setEdges],
    );

    return (
        <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            defaultEdgeOptions={defaultEdgeOptions}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onBeforeDelete={onBeforeDelete}
            onConnect={onConnect}
            fitView
        >
            <Background variant={BackgroundVariant.Dots} />
            <Controls />
        </ReactFlow>
    );
}
