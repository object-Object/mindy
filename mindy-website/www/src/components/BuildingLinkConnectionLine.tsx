import {
    getStraightPath,
    type ConnectionLineComponentProps,
} from "@xyflow/react";

export default function BuildingLinkConnectionLine({
    fromX,
    fromY,
    toX,
    toY,
    connectionLineStyle,
}: ConnectionLineComponentProps) {
    const [edgePath] = getStraightPath({
        sourceX: fromX,
        sourceY: fromY,
        targetX: toX,
        targetY: toY,
    });

    return (
        <g>
            <path
                style={{
                    stroke: "var(--mantine-color-dark-1)",
                    ...connectionLineStyle,
                }}
                fill="none"
                d={edgePath}
            />
        </g>
    );
}
