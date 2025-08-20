import { Card, Divider, Flex, Group, Text } from "@mantine/core";
import {
    Handle,
    Position,
    useConnection,
    type Node,
    type NodeProps,
} from "@xyflow/react";
import { useEffect, useState, type ReactNode } from "react";
import { TbPlugConnected } from "react-icons/tb";

import { useLogicVM } from "../../hooks";
import type { BuildingUpdateMap, VMWorkerResponse } from "../../workers/vm";
import classes from "./BuildingNode.module.css";

export type BuildingNodeData = {
    position: number;
};

type BuildingNodeType = Node<BuildingNodeData>;

interface BuildingNodeProps<K extends keyof BuildingUpdateMap>
    extends NodeProps<BuildingNodeType> {
    linkSource?: boolean;
    buildingType?: K;
    onUpdate?: (update: BuildingUpdateMap[K]) => void;
    children: ReactNode;
}

export default function BuildingNode<K extends keyof BuildingUpdateMap>({
    id,
    data: { position },
    linkSource = false,
    buildingType,
    onUpdate,
    children,
}: BuildingNodeProps<K>) {
    const vm = useLogicVM();
    const connection = useConnection();

    const [name, setName] = useState<string>();

    useEffect(() => {
        const listener = ({ data }: MessageEvent<VMWorkerResponse>) => {
            if (data.type === "ready" || data.position !== position) return;

            switch (data.type) {
                case "buildingAdded": {
                    setName(data.name);
                    break;
                }

                case "buildingUpdated": {
                    if (data.buildingType === buildingType) {
                        onUpdate?.(
                            data.update as BuildingUpdateMap[typeof buildingType],
                        );
                    }
                    break;
                }
            }
        };

        vm.addEventListener("message", listener);
        return () => {
            vm.removeEventListener("message", listener);
        };
    }, [vm, buildingType, position, onUpdate]);

    const canConnect = !connection.inProgress || connection.fromNode.id !== id;

    return (
        <>
            {canConnect && (
                <Handle
                    className={classes.targetHandle}
                    position={Position.Left}
                    type="target"
                    isConnectableStart={false}
                />
            )}
            <Card className={classes.node} withBorder radius="md">
                <Card.Section withBorder>
                    <Group justify="flex-start" align="stretch" gap={0}>
                        <Text px="sm" py={6} fw={500} ff="monospace">
                            {name ?? "unknown-building"}
                        </Text>
                        {linkSource && (
                            <>
                                <Divider orientation="vertical" ml="auto" />
                                <Flex
                                    px="sm"
                                    justify="center"
                                    align="center"
                                    pos="relative"
                                >
                                    <Handle
                                        className={classes.sourceHandle}
                                        position={Position.Right}
                                        type="source"
                                        isConnectableEnd={false}
                                    />
                                    <TbPlugConnected />
                                </Flex>
                            </>
                        )}
                    </Group>
                </Card.Section>

                {children}
            </Card>
        </>
    );
}
