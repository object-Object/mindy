import {
    BaseEdge,
    getStraightPath,
    useInternalNode,
    type EdgeProps,
} from "@xyflow/react";

import { getEdgeParams } from "../../utils";

// https://reactflow.dev/examples/nodes/easy-connect

export default function BuildingLinkEdge({
    id,
    source,
    target,
    markerEnd,
    style,
    label,
    selected,
    interactionWidth,
}: EdgeProps) {
    const sourceNode = useInternalNode(source);
    const targetNode = useInternalNode(target);

    if (!sourceNode || !targetNode) {
        return null;
    }

    const { sx, sy, tx, ty } = getEdgeParams(sourceNode, targetNode);

    const [path, labelX, labelY] = getStraightPath({
        sourceX: sx,
        sourceY: sy,
        targetX: tx,
        targetY: ty,
    });

    return (
        <BaseEdge
            id={id}
            className="react-flow__edge-path"
            path={path}
            labelX={labelX}
            labelY={labelY}
            // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
            label={(selected || sourceNode.selected) && label}
            markerEnd={markerEnd}
            style={{
                stroke:
                    selected || sourceNode.selected
                        ? "var(--mantine-color-dark-1)"
                        : "var(--mantine-color-dark-3)",
                ...style,
            }}
            labelBgBorderRadius={3}
            labelBgPadding={[2, 1]}
            labelStyle={{
                fontFamily: "monospace",
                fill: "var(--mantine-color-body)",
            }}
            interactionWidth={interactionWidth}
        />
    );
}
