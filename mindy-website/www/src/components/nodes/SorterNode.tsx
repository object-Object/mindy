import { Center, Chip, SimpleGrid } from "@mantine/core";
import type { NodeProps, Node } from "@xyflow/react";
import { useEffect, useReducer } from "react";

import { useLogicVM } from "../../hooks";
import type { BuildingNodeData } from "./BuildingNode";
import BuildingNode from "./BuildingNode";
import classes from "./SorterNode.module.css";

function reducer(
    _: string | null,
    action: number | string | null,
): string | null {
    return action?.toString() ?? null;
}

export type SorterNodeType = Node<BuildingNodeData, "sorter">;

export default function SorterNode(props: NodeProps<SorterNodeType>) {
    const {
        data: { position },
    } = props;

    const vm = useLogicVM();

    const [logicId, setLogicId] = useReducer(reducer, null);

    useEffect(() => {
        vm.postMessage({ type: "addSorter", position });

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position]);

    return (
        <BuildingNode buildingType="sorter" onUpdate={setLogicId} {...props}>
            <Center>
                <Chip.Group
                    multiple={false}
                    value={logicId}
                    onChange={(value) => {
                        setLogicId(value);
                        vm.postMessage({
                            type: "setSorterConfig",
                            position,
                            logicId: parseInt(value),
                        });
                    }}
                >
                    <SimpleGrid cols={4} spacing="xs" verticalSpacing="xs">
                        {[
                            "Copper",
                            "Lead",
                            "Metaglass",
                            "Graphite",
                            "Sand",
                            "Coal",
                            "Titanium",
                            "Thorium",
                            "Scrap",
                            "Silicon",
                            "Plastanium",
                            "Phase Fabric",
                            "Surge Alloy",
                            "Spore Pod",
                            "Blast Comp.",
                            "Pyratite",
                            "Beryllium",
                            "Tungsten",
                            "Oxide",
                            "Carbide",
                        ].map((name, i) => (
                            <Chip
                                key={name}
                                value={i.toString()}
                                className={classes.chip}
                                size="xs"
                                onClick={(e) => {
                                    if (e.currentTarget.value === logicId) {
                                        setLogicId(null);
                                        vm.postMessage({
                                            type: "setSorterConfig",
                                            position,
                                            logicId: null,
                                        });
                                    }
                                }}
                            >
                                {name}
                            </Chip>
                        ))}
                    </SimpleGrid>
                </Chip.Group>
            </Center>
        </BuildingNode>
    );
}
