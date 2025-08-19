import { Position, type InternalNode } from "@xyflow/react";

// https://reactflow.dev/examples/nodes/easy-connect

function getNodeIntersection(
    intersectionNode: InternalNode,
    targetNode: InternalNode,
) {
    // https://math.stackexchange.com/questions/1724792/an-algorithm-for-finding-the-intersection-point-between-a-center-of-vision-and-a
    const { width: intersectionNodeWidth, height: intersectionNodeHeight } =
        intersectionNode.measured;
    const intersectionNodePosition =
        intersectionNode.internals.positionAbsolute;
    const targetPosition = targetNode.internals.positionAbsolute;

    const w = (intersectionNodeWidth ?? 1) / 2;
    const h = (intersectionNodeHeight ?? 1) / 2;

    const x2 = intersectionNodePosition.x + w;
    const y2 = intersectionNodePosition.y + h;
    const x1 = targetPosition.x + (targetNode.measured.width ?? 1) / 2;
    const y1 = targetPosition.y + (targetNode.measured.height ?? 1) / 2;

    const xx1 = (x1 - x2) / (2 * w) - (y1 - y2) / (2 * h);
    const yy1 = (x1 - x2) / (2 * w) + (y1 - y2) / (2 * h);
    const a = 1 / (Math.abs(xx1) + Math.abs(yy1) || 1);
    const xx3 = a * xx1;
    const yy3 = a * yy1;
    const x = w * (xx3 + yy3) + x2;
    const y = h * (-xx3 + yy3) + y2;

    return { x, y };
}

function getEdgePosition(
    node: InternalNode,
    intersectionPoint: { x: number; y: number },
) {
    const n = { ...node.internals.positionAbsolute, ...node };
    const nx = Math.round(n.x);
    const ny = Math.round(n.y);
    const px = Math.round(intersectionPoint.x);
    const py = Math.round(intersectionPoint.y);

    if (px <= nx + 1) {
        return Position.Left;
    }
    if (px >= nx + (n.measured.width ?? 1) - 1) {
        return Position.Right;
    }
    if (py <= ny + 1) {
        return Position.Top;
    }
    if (py >= n.y + (n.measured.height ?? 1) - 1) {
        return Position.Bottom;
    }

    return Position.Top;
}

export function getEdgeParams(source: InternalNode, target: InternalNode) {
    const sourceIntersectionPoint = getNodeIntersection(source, target);
    const targetIntersectionPoint = getNodeIntersection(target, source);

    const sourcePos = getEdgePosition(source, sourceIntersectionPoint);
    const targetPos = getEdgePosition(target, targetIntersectionPoint);

    return {
        sx: sourceIntersectionPoint.x,
        sy: sourceIntersectionPoint.y,
        tx: targetIntersectionPoint.x,
        ty: targetIntersectionPoint.y,
        sourcePos,
        targetPos,
    };
}

export function packPoint(x: number, y: number): number {
    return ((y & 0xffff) << 16) | (x & 0xffff);
}

export function createNode<N, D>(
    node: N & { data: D & { position: { x: number; y: number } } },
): N & {
    id: string;
    data: D & { position: number };
} {
    const {
        position: { x, y },
    } = node.data;
    const position = packPoint(x, y);
    return {
        ...node,
        id: position.toString(),
        data: {
            ...node.data,
            position,
        },
    };
}
