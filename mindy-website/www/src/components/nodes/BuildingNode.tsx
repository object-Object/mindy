import { Card, Divider, Flex, Group, Text } from "@mantine/core";
import {
    Handle,
    Position,
    useConnection,
    type Node,
    type NodeProps,
} from "@xyflow/react";
import { type ReactNode } from "react";
import { TbPlugConnected } from "react-icons/tb";

import classes from "./BuildingNode.module.css";

export type BuildingNodeData = {
    name?: string;
    position: number;
};

type BuildingNodeType = Node<BuildingNodeData>;

interface BuildingNodeProps extends NodeProps<BuildingNodeType> {
    linkSource?: boolean;
    children: ReactNode;
}

export default function BuildingNode({
    id,
    data: { name },
    linkSource = false,
    children,
}: BuildingNodeProps) {
    const connection = useConnection();

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
