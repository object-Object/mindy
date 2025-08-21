import type { NodeProps, Node } from "@xyflow/react";
import { useEffect } from "react";

import type { MemoryKind } from "mindy-website";

import { useLogicVM } from "../../hooks";
import type { BuildingNodeData } from "./BuildingNode";
import BuildingNode from "./BuildingNode";

type MemoryNodeData = BuildingNodeData & {
    kind: MemoryKind;
};

export type MemoryNodeType = Node<MemoryNodeData, "memory">;

export default function MemoryNode(props: NodeProps<MemoryNodeType>) {
    const {
        data: { position, kind },
    } = props;

    const vm = useLogicVM();

    useEffect(() => {
        vm.postMessage({ type: "addMemory", position, kind });

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position, kind]);

    return <BuildingNode {...props} />;
}
