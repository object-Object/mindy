import { Card, Flex, Group, Text } from "@mantine/core";
import { Handle, Position, useConnection, type NodeProps } from "@xyflow/react";
import { type ReactNode } from "react";
import { TbPlugConnected } from "react-icons/tb";

import classes from "./BuildingNode.module.css";

export interface BuildingNodeProps {
    name?: string;
    linkSource?: boolean;
    children: ReactNode;
}

export default function BuildingNode({
    id,
    name,
    children,
    linkSource = false,
}: BuildingNodeProps & NodeProps) {
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
                    <Group justify="space-between" align="stretch" gap="xs">
                        <Text px="sm" py={6} fw={500} ff="monospace">
                            {name ?? "unknown-building"}
                        </Text>
                        {linkSource && (
                            <Flex
                                px="xs"
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
                        )}
                    </Group>
                </Card.Section>

                {children}
            </Card>
        </>
    );
}
